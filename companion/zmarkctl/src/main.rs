use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process;
use std::thread;
use std::time::Duration;

#[derive(Clone, Debug, PartialEq, Eq)]
struct MarkEntry {
    session: String,
    id: u128,
    pane_id: u32,
    tab_index: usize,
    top_offset: usize,
    cursor_row: usize,
    title: String,
    name: String,
    anchor: String,
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
        "Usage:\n  zellij-toolbox-zmark list [session]\n  zellij-toolbox-zmark list-machine <session>\n  zellij-toolbox-zmark add <session> <id> <pane-id> <tab-index> <top-offset> <cursor-row> <title> <name> <anchor>\n  zellij-toolbox-zmark rename <session> <id> <name>\n  zellij-toolbox-zmark delete <session> <id>"
    );
    process::exit(2);
}

fn state_dir() -> io::Result<PathBuf> {
    let dir = match env::var("XDG_STATE_HOME") {
        Ok(value) if !value.trim().is_empty() => PathBuf::from(value).join("zellij-toolbox"),
        _ => {
            let home = env::var("HOME").map_err(|_| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    "neither XDG_STATE_HOME nor HOME is available",
                )
            })?;
            PathBuf::from(home)
                .join(".local")
                .join("state")
                .join("zellij-toolbox")
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
    let path = dir.join("zmark-state.lock");

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
        "timed out waiting for zmark state lock",
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

fn read_entries(path: &Path) -> io::Result<Vec<MarkEntry>> {
    let mut data = String::new();
    match File::open(path) {
        Ok(mut file) => file.read_to_string(&mut data)?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    }

    let mut entries = Vec::new();

    for line in data.lines() {
        let fields: Vec<&str> = line.split('\t').collect();
        let [
            "M",
            session,
            id,
            pane_id,
            tab_index,
            top_offset,
            cursor_row,
            title,
            name,
            anchor,
        ] = fields.as_slice()
        else {
            continue;
        };

        let Some(session) = decode(session) else {
            continue;
        };
        let Ok(id) = id.parse::<u128>() else {
            continue;
        };
        let Ok(pane_id) = pane_id.parse::<u32>() else {
            continue;
        };
        let Ok(tab_index) = tab_index.parse::<usize>() else {
            continue;
        };
        let Ok(top_offset) = top_offset.parse::<usize>() else {
            continue;
        };
        let Ok(cursor_row) = cursor_row.parse::<usize>() else {
            continue;
        };
        let Some(title) = decode(title) else {
            continue;
        };
        let Some(name) = decode(name) else {
            continue;
        };
        let Some(anchor) = decode(anchor) else {
            continue;
        };

        entries.push(MarkEntry {
            session,
            id,
            pane_id,
            tab_index,
            top_offset,
            cursor_row,
            title,
            name,
            anchor,
        });
    }

    Ok(entries)
}

fn write_entries(dir: &Path, path: &Path, entries: &[MarkEntry]) -> io::Result<()> {
    let temp = dir.join(format!("zmark-state.{}.tmp", process::id()));

    {
        let mut file = File::create(&temp)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&temp, fs::Permissions::from_mode(0o600))?;
        }

        let mut ordered = entries.to_vec();
        ordered.sort_by(|left, right| {
            left.session
                .cmp(&right.session)
                .then(left.id.cmp(&right.id))
        });

        for entry in ordered {
            writeln!(
                file,
                "M\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                encode(&entry.session),
                entry.id,
                entry.pane_id,
                entry.tab_index,
                entry.top_offset,
                entry.cursor_row,
                encode(&entry.title),
                encode(&entry.name),
                encode(&entry.anchor)
            )?;
        }

        file.sync_all()?;
    }

    fs::rename(temp, path)
}

fn with_locked_entries<T>(
    action: impl FnOnce(&Path, &Path, &mut Vec<MarkEntry>) -> io::Result<T>,
) -> io::Result<T> {
    let dir = state_dir()?;
    let state = dir.join("zmark-state.tsv");
    let _lock = acquire_lock(&dir)?;
    let mut entries = read_entries(&state)?;
    action(&dir, &state, &mut entries)
}

fn add(args: &[String]) -> io::Result<()> {
    if args.len() != 9 {
        usage();
    }

    let entry = MarkEntry {
        session: args[0].clone(),
        id: args[1]
            .parse::<u128>()
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid mark id"))?,
        pane_id: args[2]
            .parse::<u32>()
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid pane id"))?,
        tab_index: args[3]
            .parse::<usize>()
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid tab index"))?,
        top_offset: args[4]
            .parse::<usize>()
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid top offset"))?,
        cursor_row: args[5]
            .parse::<usize>()
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid cursor row"))?,
        title: args[6].clone(),
        name: args[7].clone(),
        anchor: args[8].clone(),
    };

    with_locked_entries(|dir, state, entries| {
        entries.retain(|existing| {
            !(existing.session == entry.session && existing.id == entry.id)
        });
        entries.push(entry);
        write_entries(dir, state, entries)
    })
}

fn rename(args: &[String]) -> io::Result<()> {
    if args.len() != 3 {
        usage();
    }

    let session = &args[0];
    let id = args[1]
        .parse::<u128>()
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid mark id"))?;
    let name = &args[2];

    with_locked_entries(|dir, state, entries| {
        let Some(entry) = entries
            .iter_mut()
            .find(|entry| entry.session == *session && entry.id == id)
        else {
            return Err(io::Error::new(io::ErrorKind::NotFound, "mark not found"));
        };

        entry.name = name.clone();
        write_entries(dir, state, entries)
    })
}

fn delete(args: &[String]) -> io::Result<()> {
    if args.len() != 2 {
        usage();
    }

    let session = &args[0];
    let id = args[1]
        .parse::<u128>()
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid mark id"))?;

    with_locked_entries(|dir, state, entries| {
        entries.retain(|entry| !(entry.session == *session && entry.id == id));
        write_entries(dir, state, entries)
    })
}

fn list_entries(session: Option<&str>) -> io::Result<Vec<MarkEntry>> {
    with_locked_entries(|_dir, _state, entries| {
        let mut selected: Vec<MarkEntry> = entries
            .iter()
            .filter(|entry| session.map(|name| entry.session == name).unwrap_or(true))
            .cloned()
            .collect();

        selected.sort_by(|left, right| {
            left.session
                .cmp(&right.session)
                .then(left.id.cmp(&right.id))
        });
        Ok(selected)
    })
}

fn print_machine(entries: &[MarkEntry]) {
    for entry in entries {
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            entry.id,
            entry.pane_id,
            entry.tab_index,
            entry.top_offset,
            entry.cursor_row,
            encode(&entry.title),
            encode(&entry.name),
            encode(&entry.anchor)
        );
    }
}

fn print_human(entries: &[MarkEntry]) {
    if entries.is_empty() {
        println!("No durable zmarks.");
        return;
    }

    for (index, entry) in entries.iter().enumerate() {
        let name = if entry.name.is_empty() {
            "(unnamed)"
        } else {
            &entry.name
        };
        println!(
            "{:>2}. {} P{} T{} {} — {}",
            index + 1,
            entry.session,
            entry.pane_id,
            entry.tab_index + 1,
            name,
            entry.title
        );
    }
}

fn run() -> io::Result<()> {
    let mut args = env::args().skip(1);
    let Some(command) = args.next() else {
        usage();
    };
    let rest: Vec<String> = args.collect();

    match command.as_str() {
        "add" => add(&rest),
        "rename" => rename(&rest),
        "delete" => delete(&rest),
        "list-machine" => {
            if rest.len() != 1 {
                usage();
            }
            let entries = list_entries(Some(&rest[0]))?;
            print_machine(&entries);
            Ok(())
        }
        "list" => {
            if rest.len() > 1 {
                usage();
            }
            let entries = list_entries(rest.first().map(String::as_str))?;
            print_human(&entries);
            Ok(())
        }
        _ => usage(),
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("zellij-toolbox-zmark: {error}");
        process::exit(1);
    }
}
