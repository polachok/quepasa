use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Error};
use axum::extract::Path as Route;
use axum::{extract::Extension, response::Json, routing::get, AddExtensionLayer, Router};
use keepass::{Database, NodeRef};
use serde::{Deserialize, Serialize};
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt::init();

    let first_arg = std::env::args().nth(1).clone();
    let path = match first_arg {
        Some(arg) => Path::new(&arg).to_owned(),
        None => anyhow::bail!("usage: quepasa <db.kdbx>"),
    };
    let mut file = File::open(path)?;
    let pass = rpassword::prompt_password_stdout("Password: ")?;
    let db = Database::open(&mut file, Some(&pass), None)?;
    let db = Arc::new(db);

    info!("opened db");

    let dirs = xdg::BaseDirectories::new()?;
    let rt_dir = dirs.get_runtime_directory()?;
    let socket_path = rt_dir.join("quepasa.sock");
    let pid_path = rt_dir.join("quepasa.pid");

    check_running(&pid_path)?;
    let _ = std::fs::remove_file(&socket_path);
    let _ = std::fs::remove_file(&pid_path);
    write_pid(&pid_path)?;

    info!("listening on {}", socket_path.display());
    serve(db, socket_path).await?;

    Ok(())
}

fn check_running(path: &Path) -> Result<(), Error> {
    use std::io::Read;

    let mut f = match File::open(path) {
        Ok(f) => f,
        Err(_) => return Ok(()),
    };
    let mut buf = String::new();

    f.read_to_string(&mut buf)?;
    let pid: u32 = buf.parse()?;
    let rv = unsafe { libc::kill(pid as i32, 0) };
    if rv == 0 {
        anyhow::bail!("already running, pid: {}", pid);
    }
    Ok(())
}

fn write_pid(path: &Path) -> Result<(), Error> {
    use std::io::Write;

    let mut f = File::create(path).with_context(|| "failed to write pid")?;
    write!(&mut f, "{}", std::process::id())?;

    Ok(())
}

#[derive(Clone)]
struct State {
    db: Arc<Database>,
}

impl State {
    fn walk(group: &keepass::Group, depth: u32, mut f: impl FnMut(NodeRef<'_>, u32) -> bool) {
        for child in &group.children {
            let child = child.to_ref();
            match child {
                NodeRef::Group(group) => {
                    if f(child, depth) {
                        return Self::walk(group, depth + 1, f);
                    }
                }
                NodeRef::Entry(_) => {
                    f(child, depth);
                }
            }
        }
    }

    fn list(&self, path: &[&str]) -> Vec<String> {
        let mut list = vec![];

        Self::walk(&self.db.root, 0, |node, depth| {
            let want = path.get(depth as usize);
            match node {
                NodeRef::Group(group) => {
                    let name = group.name.as_str();
                    if want.is_none() {
                        list.push(format!("{}/", name));
                        return false;
                    }
                    Some(&name) == want
                }
                NodeRef::Entry(entry) => {
                    let name = entry.get_title();
                    if want.is_none() || want == name.as_ref() {
                        list.push(entry.get_title().unwrap().to_owned());
                    }
                    false
                }
            }
        });
        list
    }

    fn get(&self, path: &[&str], method: Method) -> Option<String> {
        let node = self.db.root.get(path)?;
        match node {
            NodeRef::Group(_) => {}
            NodeRef::Entry(e) => {
                return match method {
                    Method::GetUsername => e.get_username().map(|s| s.to_owned()),
                    Method::GetPassword => e.get_password().map(|s| s.to_owned()),
                }
            }
        }
        None
    }
}

#[derive(Serialize, Deserialize)]
enum Method {
    GetUsername,
    GetPassword,
}

#[derive(Serialize, Deserialize)]
struct Request {
    path: String,
    method: Method,
}

async fn list(Route(path): Route<String>, state: Extension<State>) -> Json<Vec<String>> {
    let path: Vec<&str> = path.split('/').skip(1).filter(|s| !s.is_empty()).collect();
    Json(state.list(&path))
}

async fn get_attr(Json(req): Json<Request>, state: Extension<State>) -> Json<Option<String>> {
    let path: Vec<&str> = req
        .path
        .split('/')
        .skip(1)
        .filter(|s| !s.is_empty())
        .collect();
    Json(state.get(&path, req.method))
}

async fn serve(db: Arc<Database>, socket_path: PathBuf) -> Result<(), Error> {
    use hyperlocal::UnixServerExt;

    let state = State { db };
    let app = Router::new()
        .route("/*path", get(list).post(get_attr))
        .layer(AddExtensionLayer::new(state));

    axum::Server::bind_unix(socket_path)?
        .serve(app.into_make_service())
        .await?;
    Ok(())
}
