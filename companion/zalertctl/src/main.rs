use std::collections::{BTreeMap, HashSet};
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{self, Command};
use std::thread;
use std::time::Duration;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Entry {
    session: String,
    pane_id: u32,
    generation: u128,
    revision: u64,
    phase: String,
    title: String,
    command: String,
}

#[derive(Clone, Debug, Default)]
struct Snapshot {
    session_generations: BTreeMap<String, u128>,
    entries: Vec<Entry>,
}

struct StateLock {
    path: PathBuf,
}

impl Drop for StateLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn usage() -> ! {
    eprintln!(
        "Usage:\n  zellij-toolbox-alert list\n  zellij-toolbox-alert jump <index>\n  zellij-toolbox-alert touch <session> <generation>\n  zellij-toolbox-alert upsert <session> <pane-id> <generation> <revision> <armed|running> <title> <command>\n  zellij-toolbox-alert clear <session> <pane-id> <generation> <revision>\n  zellij-toolbox-alert prune"
    );
    process::exit(2);
}

fn state_dir() -> io::Result<PathBuf> {
    let dir = match env::var("XDG_RUNTIME_DIR") {
        Ok(value) if !value.trim().is_empty() => PathBuf::from(value).join("zellij-toolbox"),
        _ => {
            let home = env::var("HOME").map_err(|_| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    "neither XDG_RUNTIME_DIR nor HOME is available",
                )
            })?;
            PathBuf::from(home)
                .join(".local")
                .join("state")
                .join("zellij-toolbox")
                .join("runtime")
        }
    };

    fs::create_dir_all(&dir)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))?;
    }

    Ok(dir)
}

fn acquire_lock(dir: &Path) -> io::Result<StateLock> {
    let path = dir.join("zalert-state.lock");

    for _ in 0..200 {
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                writeln!(file, "{}", process::id())?;
                return Ok(StateLock { path });
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                let stale = fs::metadata(&path)
                    .and_then(|metadata| metadata.modified())
                    .and_then(|modified| modified.elapsed().map_err(io::Error::other))
                    .map(|age| age > Duration::from_secs(10))
                    .unwrap_or(false);

                if stale {
                    let _ = fs::remove_file(&path);
                    continue;
                }

                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => return Err(error),
        }
    }

    Err(io::Error::new(
        io::ErrorKind::TimedOut,
        "timed out waiting for zalert state lock",
    ))
}

fn encode(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len() * 2);
    for byte in value.as_bytes() {
        use std::fmt::Write as _;
        let _ = write!(encoded, "{byte:02x}");
    }
    encoded
}

fn decode(value: &str) -> Option<String> {
    if value.len() % 2 != 0 {
        return None;
    }

    let mut bytes = Vec::with_capacity(value.len() / 2);
    let mut index = 0;
    while index < value.len() {
        let byte = u8::from_str_radix(&value[index..index + 2], 16).ok()?;
        bytes.push(byte);
        index += 2;
    }

    String::from_utf8(bytes).ok()
}

fn read_snapshot(path: &Path) -> io::Result<Snapshot> {
    let mut data = String::new();
    match File::open(path) {
        Ok(mut file) => {
            file.read_to_string(&mut data)?;
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Snapshot::default()),
        Err(error) => return Err(error),
    }

    let mut snapshot = Snapshot::default();

    for line in data.lines() {
        let fields: Vec<&str> = line.split('\t').collect();
        match fields.as_slice() {
            ["S", session, generation] => {
                let Some(session) = decode(session) else {
                    continue;
                };
                let Ok(generation) = generation.parse::<u128>() else {
                    continue;
                };
                snapshot.session_generations.insert(session, generation);
            }
            [
                "W",
                session,
                pane_id,
                generation,
                revision,
                phase,
                title,
                command,
            ] => {
                let Some(session) = decode(session) else {
                    continue;
                };
                let Ok(pane_id) = pane_id.parse::<u32>() else {
                    continue;
                };
                let Ok(generation) = generation.parse::<u128>() else {
                    continue;
                };
                let Ok(revision) = revision.parse::<u64>() else {
                    continue;
                };
                if !matches!(*phase, "armed" | "running" | "cleared") {
                    continue;
                }
                let Some(title) = decode(title) else {
                    continue;
                };
                let Some(command) = decode(command) else {
                    continue;
                };

                snapshot.entries.push(Entry {
                    session,
                    pane_id,
                    generation,
                    revision,
                    phase: (*phase).to_owned(),
                    title,
                    command,
                });
            }
            _ => {}
        }
    }

    Ok(snapshot)
}

fn write_snapshot(dir: &Path, path: &Path, snapshot: &Snapshot) -> io::Result<()> {
    let temp = dir.join(format!("zalert-state.{}.tmp", process::id()));

    {
        let mut file = File::create(&temp)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&temp, fs::Permissions::from_mode(0o600))?;
        }

        for (session, generation) in &snapshot.session_generations {
            writeln!(file, "S\t{}\t{}", encode(session), generation)?;
        }

        let mut entries = snapshot.entries.clone();
        entries.sort_by(|left, right| {
            left.session
                .cmp(&right.session)
                .then(left.pane_id.cmp(&right.pane_id))
        });

        for entry in entries {
            writeln!(
                file,
                "W\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                encode(&entry.session),
                entry.pane_id,
                entry.generation,
                entry.revision,
                entry.phase,
                encode(&entry.title),
                encode(&entry.command)
            )?;
        }

        file.sync_all()?;
    }

    fs::rename(temp, path)
}

fn active_sessions() -> Option<HashSet<String>> {
    let output = Command::new("zellij")
        .args(["list-sessions", "--short"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8(output.stdout).ok()?;
    Some(
        stdout
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(ToOwned::to_owned)
            .collect(),
    )
}

fn prune(snapshot: &mut Snapshot) -> bool {
    let Some(active) = active_sessions() else {
        return false;
    };

    let before_entries = snapshot.entries.len();
    let before_sessions = snapshot.session_generations.len();

    snapshot
        .entries
        .retain(|entry| active.contains(&entry.session));
    snapshot
        .session_generations
        .retain(|session, _| active.contains(session));

    snapshot.entries.len() != before_entries
        || snapshot.session_generations.len() != before_sessions
}

fn with_locked_snapshot<T>(
    action: impl FnOnce(&Path, &Path, &mut Snapshot) -> io::Result<T>,
) -> io::Result<T> {
    let dir = state_dir()?;
    let state = dir.join("zalert-state.tsv");
    let _lock = acquire_lock(&dir)?;
    let mut snapshot = read_snapshot(&state)?;
    action(&dir, &state, &mut snapshot)
}

fn advance_session(snapshot: &mut Snapshot, session: &str, generation: u128) -> bool {
    match snapshot.session_generations.get(session).copied() {
        Some(current) if current > generation => false,
        Some(current) if current == generation => true,
        _ => {
            snapshot
                .session_generations
                .insert(session.to_owned(), generation);
            snapshot
                .entries
                .retain(|entry| entry.session != session || entry.generation >= generation);
            true
        }
    }
}

fn touch(args: &[String]) -> io::Result<()> {
    if args.len() != 2 {
        usage();
    }

    let session = &args[0];
    let generation = args[1]
        .parse::<u128>()
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid generation"))?;

    with_locked_snapshot(|dir, state, snapshot| {
        if advance_session(snapshot, session, generation) {
            write_snapshot(dir, state, snapshot)?;
        }
        Ok(())
    })
}

fn upsert(args: &[String]) -> io::Result<()> {
    if args.len() != 7 {
        usage();
    }

    let session = args[0].clone();
    let pane_id = args[1]
        .parse::<u32>()
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid pane id"))?;
    let generation = args[2]
        .parse::<u128>()
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid generation"))?;
    let revision = args[3]
        .parse::<u64>()
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid revision"))?;
    let phase = args[4].clone();
    if phase != "armed" && phase != "running" {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "phase must be armed or running",
        ));
    }

    let title = args[5].clone();
    let command = args[6].clone();

    with_locked_snapshot(|dir, state, snapshot| {
        if !advance_session(snapshot, &session, generation) {
            return Ok(());
        }

        let should_apply = snapshot
            .entries
            .iter()
            .find(|entry| entry.session == session && entry.pane_id == pane_id)
            .map(|entry| {
                entry.generation < generation
                    || (entry.generation == generation && entry.revision < revision)
            })
            .unwrap_or(true);

        if !should_apply {
            return Ok(());
        }

        snapshot
            .entries
            .retain(|entry| !(entry.session == session && entry.pane_id == pane_id));
        snapshot.entries.push(Entry {
            session,
            pane_id,
            generation,
            revision,
            phase,
            title,
            command,
        });
        write_snapshot(dir, state, snapshot)
    })
}

fn clear(args: &[String]) -> io::Result<()> {
    if args.len() != 4 {
        usage();
    }

    let session = args[0].clone();
    let pane_id = args[1]
        .parse::<u32>()
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid pane id"))?;
    let generation = args[2]
        .parse::<u128>()
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid generation"))?;
    let revision = args[3]
        .parse::<u64>()
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid revision"))?;

    with_locked_snapshot(|dir, state, snapshot| {
        if !advance_session(snapshot, &session, generation) {
            return Ok(());
        }

        let should_apply = snapshot
            .entries
            .iter()
            .find(|entry| entry.session == session && entry.pane_id == pane_id)
            .map(|entry| {
                entry.generation < generation
                    || (entry.generation == generation && entry.revision < revision)
            })
            .unwrap_or(true);

        if !should_apply {
            return Ok(());
        }

        snapshot
            .entries
            .retain(|entry| !(entry.session == session && entry.pane_id == pane_id));
        snapshot.entries.push(Entry {
            session,
            pane_id,
            generation,
            revision,
            phase: "cleared".to_owned(),
            title: String::new(),
            command: String::new(),
        });
        write_snapshot(dir, state, snapshot)
    })
}

fn list_entries() -> io::Result<Vec<Entry>> {
    with_locked_snapshot(|dir, state, snapshot| {
        if prune(snapshot) {
            write_snapshot(dir, state, snapshot)?;
        }

        let mut entries: Vec<Entry> = snapshot
            .entries
            .iter()
            .filter(|entry| entry.phase != "cleared")
            .cloned()
            .collect();

        entries.sort_by(|left, right| {
            left.session
                .cmp(&right.session)
                .then(left.pane_id.cmp(&right.pane_id))
        });

        Ok(entries)
    })
}

fn print_entries(entries: &[Entry]) {
    if entries.is_empty() {
        println!("No active zalert watches.");
        return;
    }

    for (index, entry) in entries.iter().enumerate() {
        let detail = if entry.phase == "armed" {
            "waiting for next command".to_owned()
        } else if entry.command.is_empty() {
            "running command".to_owned()
        } else {
            entry.command.clone()
        };

        println!(
            "{:>2}. {:<20} P{:<4} {:<7} {} — {}",
            index + 1,
            entry.session,
            entry.pane_id,
            entry.phase,
            entry.title,
            detail
        );
    }
}

fn jump(args: &[String]) -> io::Result<()> {
    if args.len() != 1 {
        usage();
    }

    let index = args[0]
        .parse::<usize>()
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid watch index"))?;

    if index == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "watch index starts at 1",
        ));
    }

    let entries = list_entries()?;
    let Some(entry) = entries.get(index - 1) else {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("no watch at index {index}"),
        ));
    };

    let pane = format!("terminal_{}", entry.pane_id);
    let current_session = env::var("ZELLIJ_SESSION_NAME").map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "jump must be run from inside an active Zellij client",
        )
    })?;

    let status = if current_session == entry.session {
        Command::new("zellij")
            .args(["action", "focus-pane-id", &pane])
            .status()?
    } else {
        Command::new("zellij")
            .args([
                "action",
                "switch-session",
                &entry.session,
                "--pane-id",
                &pane,
            ])
            .status()?
    };

    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "zellij returned exit status {status}"
        )))
    }
}

fn prune_command() -> io::Result<()> {
    with_locked_snapshot(|dir, state, snapshot| {
        if prune(snapshot) {
            write_snapshot(dir, state, snapshot)?;
        }
        Ok(())
    })
}

fn run() -> io::Result<()> {
    let mut args = env::args().skip(1);
    let Some(command) = args.next() else {
        usage();
    };
    let rest: Vec<String> = args.collect();

    match command.as_str() {
        "touch" => touch(&rest),
        "upsert" => upsert(&rest),
        "clear" => clear(&rest),
        "list" => {
            let entries = list_entries()?;
            print_entries(&entries);
            Ok(())
        }
        "jump" => jump(&rest),
        "prune" => prune_command(),
        _ => usage(),
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("zellij-toolbox-alert: {error}");
        process::exit(1);
    }
}
