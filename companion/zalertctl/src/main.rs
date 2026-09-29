use std::collections::HashSet;
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
    phase: String,
    title: String,
    command: String,
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
        "Usage:\n  zellij-toolbox-alert list\n  zellij-toolbox-alert jump <index>\n  zellij-toolbox-alert upsert <session> <pane-id> <armed|running> <title> <command>\n  zellij-toolbox-alert clear <session> <pane-id>\n  zellij-toolbox-alert clear-session <session>\n  zellij-toolbox-alert prune"
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

fn read_entries(path: &Path) -> io::Result<Vec<Entry>> {
    let mut data = String::new();
    match File::open(path) {
        Ok(mut file) => {
            file.read_to_string(&mut data)?;
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    }

    let mut entries = Vec::new();

    for line in data.lines() {
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() != 5 {
            continue;
        }

        let Some(session) = decode(fields[0]) else {
            continue;
        };
        let Ok(pane_id) = fields[1].parse::<u32>() else {
            continue;
        };
        let phase = fields[2].to_owned();
        if phase != "armed" && phase != "running" {
            continue;
        }
        let Some(title) = decode(fields[3]) else {
            continue;
        };
        let Some(command) = decode(fields[4]) else {
            continue;
        };

        entries.push(Entry {
            session,
            pane_id,
            phase,
            title,
            command,
        });
    }

    Ok(entries)
}

fn write_entries(dir: &Path, path: &Path, entries: &[Entry]) -> io::Result<()> {
    let temp = dir.join(format!("zalert-state.{}.tmp", process::id()));

    {
        let mut file = File::create(&temp)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&temp, fs::Permissions::from_mode(0o600))?;
        }

        for entry in entries {
            writeln!(
                file,
                "{}\t{}\t{}\t{}\t{}",
                encode(&entry.session),
                entry.pane_id,
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

fn prune(entries: &mut Vec<Entry>) -> bool {
    let Some(active) = active_sessions() else {
        return false;
    };

    let before = entries.len();
    entries.retain(|entry| active.contains(&entry.session));
    entries.len() != before
}

fn with_locked_entries<T>(
    action: impl FnOnce(&Path, &Path, &mut Vec<Entry>) -> io::Result<T>,
) -> io::Result<T> {
    let dir = state_dir()?;
    let state = dir.join("zalert-state.tsv");
    let _lock = acquire_lock(&dir)?;
    let mut entries = read_entries(&state)?;
    action(&dir, &state, &mut entries)
}

fn upsert(args: &[String]) -> io::Result<()> {
    if args.len() != 5 {
        usage();
    }

    let session = args[0].clone();
    let pane_id = args[1]
        .parse::<u32>()
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid pane id"))?;
    let phase = args[2].clone();
    if phase != "armed" && phase != "running" {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "phase must be armed or running",
        ));
    }

    let title = args[3].clone();
    let command = args[4].clone();

    with_locked_entries(|dir, state, entries| {
        entries.retain(|entry| !(entry.session == session && entry.pane_id == pane_id));
        entries.push(Entry {
            session,
            pane_id,
            phase,
            title,
            command,
        });
        entries.sort_by(|left, right| {
            left.session
                .cmp(&right.session)
                .then(left.pane_id.cmp(&right.pane_id))
        });
        write_entries(dir, state, entries)
    })
}

fn clear(args: &[String]) -> io::Result<()> {
    if args.len() != 2 {
        usage();
    }

    let session = &args[0];
    let pane_id = args[1]
        .parse::<u32>()
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid pane id"))?;

    with_locked_entries(|dir, state, entries| {
        entries.retain(|entry| !(entry.session == *session && entry.pane_id == pane_id));
        write_entries(dir, state, entries)
    })
}

fn clear_session(args: &[String]) -> io::Result<()> {
    if args.len() != 1 {
        usage();
    }

    let session = &args[0];

    with_locked_entries(|dir, state, entries| {
        entries.retain(|entry| entry.session != *session);
        write_entries(dir, state, entries)
    })
}

fn list_entries() -> io::Result<Vec<Entry>> {
    with_locked_entries(|dir, state, entries| {
        if prune(entries) {
            write_entries(dir, state, entries)?;
        }
        Ok(entries.clone())
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
    let current_session = env::var("ZELLIJ_SESSION_NAME").ok();

    let status = if current_session.as_deref() == Some(entry.session.as_str()) {
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
    with_locked_entries(|dir, state, entries| {
        if prune(entries) {
            write_entries(dir, state, entries)?;
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
        "upsert" => upsert(&rest),
        "clear" => clear(&rest),
        "clear-session" => clear_session(&rest),
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
