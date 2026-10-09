//! `machie session ...` — create, list, show, add notes to, and delete sessions.
//! Sessions are the container v0.2+ will use for structured tasks and chat history.

use clap::Subcommand;
use machie_db::Db;

#[derive(Subcommand)]
pub enum SessionCmd {
    /// Create a new session with an optional title
    New {
        /// Title for the session
        #[arg(default_value = "")]
        title: String,
    },
    /// List all sessions (newest first)
    List,
    /// Show one session's messages
    Show {
        /// Session id (see `machie session list`)
        id: i64,
    },
    /// Append a note (as the user) to a session
    Note {
        /// Session id
        id: i64,
        /// The note text
        text: String,
    },
    /// Delete a session and all its messages
    Delete {
        /// Session id
        id: i64,
    },
}

pub fn run(cmd: SessionCmd, db: &Db) -> Result<(), Box<dyn std::error::Error>> {
    match cmd {
        SessionCmd::New { title } => {
            let title = if title.is_empty() { "untitled" } else { &title };
            let id = db.create_session(title)?;
            println!("Created session #{id} \"{title}\"");
        }
        SessionCmd::List => {
            let rows = db.list_sessions()?;
            if rows.is_empty() {
                println!("No sessions yet. Try: machie session new \"my first\"");
            } else {
                for (id, title, created, count) in rows {
                    let title = if title.is_empty() { "untitled" } else { &title };
                    println!("#{id:<4} {count:>3} msg(s)  {created}  {title}");
                }
            }
        }
        SessionCmd::Show { id } => {
            let msgs = db.session_messages(id)?;
            let exists = db.list_sessions()?.iter().any(|(sid, ..)| *sid == id);
            if !exists {
                return Err(format!("no session with id {id} (try `machie session list`)").into());
            }
            if msgs.is_empty() {
                println!("Session #{id} has no messages yet.");
            }
            for (role, content) in msgs {
                println!("[{role}] {content}");
            }
        }
        SessionCmd::Note { id, text } => {
            let exists = db.list_sessions()?.iter().any(|(sid, ..)| *sid == id);
            if !exists {
                return Err(format!("no session with id {id} (try `machie session list`)").into());
            }
            let mid = db.add_message(id, "user", &text)?;
            println!("Added message #{mid} to session #{id}");
        }
        SessionCmd::Delete { id } => {
            let n = db.delete_session(id)?;
            if n == 0 {
                return Err(format!("no session with id {id} (nothing deleted)").into());
            }
            println!("Deleted session #{id} and its messages");
        }
    }
    Ok(())
}
