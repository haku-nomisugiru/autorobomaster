use crate::{agent, api, domain::*, AppState};
use axum::{
    extract::{DefaultBodyLimit, State},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
    Json, Router,
};
use qrcode::{render::svg, QrCode};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    sync::Mutex,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::{oneshot, Mutex as AsyncMutex};

pub const PORT: u16 = 49731;
#[derive(Default)]
pub struct Mobile {
    listener: AsyncMutex<Option<oneshot::Sender<()>>>,
    pairing: Mutex<Option<Ticket>>,
}
struct Ticket {
    code: String,
    project: String,
    expires: Instant,
    read_only: bool,
}
type Error = (StatusCode, String);
type Reply = Result<Json<Value>, Error>;
fn internal(e: impl std::fmt::Display) -> Error {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}
fn conflict(s: &str) -> Error {
    (StatusCode::CONFLICT, s.into())
}
fn forbidden() -> Error {
    (StatusCode::FORBIDDEN, "设备未获当前项目授权".into())
}
fn fingerprint(s: &str) -> String {
    format!("{:x}", Sha256::digest(s.as_bytes()))
}

pub fn initialize(db: &crate::store::Store) -> Result<(), String> {
    db.conn.execute_batch("CREATE TABLE IF NOT EXISTS mobile_devices (token TEXT PRIMARY KEY,project TEXT NOT NULL,name TEXT NOT NULL,mode TEXT NOT NULL);
    DELETE FROM mobile_devices WHERE mode LIKE 'pending%';
    CREATE TABLE IF NOT EXISTS mobile_previews (id TEXT PRIMARY KEY,project TEXT NOT NULL,stage TEXT NOT NULL,iteration INTEGER,created TEXT NOT NULL,image TEXT NOT NULL);
    CREATE TABLE IF NOT EXISTS mobile_ops (id TEXT PRIMARY KEY,project TEXT NOT NULL,payload TEXT NOT NULL,status TEXT NOT NULL,detail TEXT NOT NULL);
    UPDATE mobile_ops SET status='unknown',detail='电脑在任务期间退出，请先核对实际结果' WHERE status='running';").map_err(|e|e.to_string())
}
#[tauri::command]
pub async fn mobile_enable(app: AppHandle) -> Result<(), String> {
    let mobile = app.state::<Mobile>();
    let mut running = mobile.listener.lock().await;
    if running.is_some() {
        return Ok(());
    }
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", PORT))
        .await
        .map_err(|e| format!("无法打开端口 {PORT}：{e}"))?;
    let (shutdown, rx) = oneshot::channel();
    let router = Router::new()
        .route("/v1/pair", post(pair))
        .route("/v1/state", get(snapshot))
        .route("/v1/operation", post(operation))
        .route("/v1/stop", post(stop))
        .route("/v1/preview", post(preview))
        .layer(DefaultBodyLimit::max(64 * 1024))
        .with_state(app.clone());
    *running = Some(shutdown);
    tauri::async_runtime::spawn(async move {
        let _ = axum::serve(listener, router)
            .with_graceful_shutdown(async {
                let _ = rx.await;
            })
            .await;
    });
    Ok(())
}
#[tauri::command]
pub async fn mobile_disable(app: AppHandle) -> Result<(), String> {
    let mobile = app.state::<Mobile>();
    let mut running = mobile.listener.lock().await;
    if let Some(tx) = running.take() {
        let _ = tx.send(());
    }
    *mobile.pairing.lock().unwrap() = None;
    let state = app.state::<AppState>();
    state
        .db
        .lock()
        .unwrap()
        .conn
        .execute("DELETE FROM mobile_devices", [])
        .map_err(|e| e.to_string())?;
    Ok(())
}
#[tauri::command]
pub async fn mobile_ticket(
    app: AppHandle,
    address: String,
    read_only: bool,
) -> Result<Value, String> {
    let url = url::Url::parse(address.trim()).map_err(|_| "请输入完整的 HTTPS 访问地址")?;
    let usb =
        url.scheme() == "http" && url.host_str() == Some("127.0.0.1") && url.port().is_some_and(|p|p>=1024);
    if (!usb && url.scheme() != "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || url.query().is_some()
        || url.path() != "/"
    {
        return Err("使用专网 HTTPS 根地址，USB 调试使用 http://127.0.0.1:49731".into());
    }
    mobile_enable(app.clone()).await?;
    let project = app.state::<AppState>().db.lock().unwrap().active()?.id;
    let code = id();
    let payload =
        json!({"studio":1,"url":url.origin().ascii_serialization(),"ticket":code}).to_string();
    let image = QrCode::new(payload.as_bytes())
        .map_err(|e| e.to_string())?
        .render::<svg::Color>()
        .min_dimensions(260, 260)
        .build();
    let mobile = app.state::<Mobile>();
    *mobile.pairing.lock().unwrap() = Some(Ticket {
        code,
        project,
        expires: Instant::now() + Duration::from_secs(120),
        read_only,
    });
    Ok(json!({"svg":image,"payload":payload}))
}
#[tauri::command]
pub async fn mobile_admin(app: AppHandle) -> Result<Value, String> {
    let enabled = app.state::<Mobile>().listener.lock().await.is_some();
    let state = app.state::<AppState>();
    let db = state.db.lock().unwrap();
    let mut stmt = db
        .conn
        .prepare("SELECT token,name,mode,project FROM mobile_devices ORDER BY rowid DESC")
        .map_err(|e| e.to_string())?;
    let rows=stmt.query_map([],|r|Ok(json!({"id":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?,"mode":r.get::<_,String>(2)?,"project":r.get::<_,String>(3)?}))).map_err(|e|e.to_string())?;
    Ok(
        json!({"enabled":enabled,"devices":rows.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?}),
    )
}
#[tauri::command]
pub fn mobile_device(app: AppHandle, device: String, approve: bool) -> Result<(), String> {
    let state = app.state::<AppState>();
    let db = state.db.lock().unwrap();
    let sql = if approve {
        "UPDATE mobile_devices SET mode=substr(mode,9) WHERE token=?1 AND mode LIKE 'pending-%'"
    } else {
        "DELETE FROM mobile_devices WHERE token=?1"
    };
    db.conn.execute(sql, [device]).map_err(|e| e.to_string())?;
    Ok(())
}
#[derive(Deserialize)]
struct Pair {
    ticket: String,
    token: String,
    name: String,
}
async fn pair(State(app): State<AppHandle>, Json(req): Json<Pair>) -> Reply {
    if uuid::Uuid::parse_str(&req.token).is_err() || req.name.trim().is_empty() {
        return Err(forbidden());
    }
    let mobile = app.state::<Mobile>();
    let mut slot = mobile.pairing.lock().unwrap();
    let t = slot
        .as_ref()
        .filter(|t| t.expires > Instant::now() && t.code == req.ticket)
        .ok_or_else(forbidden)?;
    let state = app.state::<AppState>();
    let db = state.db.lock().unwrap();
    db.conn
        .execute(
            "INSERT INTO mobile_devices(token,project,name,mode) VALUES(?1,?2,?3,?4)",
            params![
                fingerprint(&req.token),
                t.project,
                req.name.chars().take(64).collect::<String>(),
                if t.read_only {
                    "pending-read"
                } else {
                    "pending-control"
                }
            ],
        )
        .map_err(internal)?;
    *slot = None;
    Ok(Json(json!({"pending":true})))
}
fn access(db: &crate::store::Store, h: &HeaderMap) -> Result<(String, String), Error> {
    let token = h
        .get("authorization")
        .and_then(|x| x.to_str().ok())
        .and_then(|x| x.strip_prefix("Bearer "))
        .ok_or_else(forbidden)?;
    db.conn
        .query_row(
            "SELECT project,mode FROM mobile_devices WHERE token=?1",
            [fingerprint(token)],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(internal)?
        .ok_or_else(forbidden)
}
async fn snapshot(State(app): State<AppHandle>, h: HeaderMap) -> Reply {
    let state = app.state::<AppState>();
    let db = state.db.lock().unwrap();
    let (project, mode) = access(&db, &h)?;
    if mode.starts_with("pending-") {
        return Ok(Json(json!({"pendingApproval":true})));
    }
    let p = db.project(&project).map_err(internal)?;
    let last:Option<Value>=db.conn.query_row("SELECT id,status,detail FROM mobile_ops WHERE project=?1 ORDER BY rowid DESC LIMIT 1",[&project],|r|Ok(json!({"id":r.get::<_,String>(0)?,"status":r.get::<_,String>(1)?,"detail":r.get::<_,String>(2)?}))).optional().map_err(internal)?;
    let mut stmt=db.conn.prepare("SELECT id,stage,iteration,created FROM mobile_previews WHERE project=?1 ORDER BY rowid DESC LIMIT 12").map_err(internal)?;
    let previews=stmt.query_map([&project],|r|Ok(json!({"id":r.get::<_,String>(0)?,"stage":r.get::<_,String>(1)?,"iteration":r.get::<_,Option<usize>>(2)?,"created":r.get::<_,String>(3)?}))).map_err(internal)?.collect::<Result<Vec<_>,_>>().map_err(internal)?;
    Ok(Json(
        json!({"project":p.id,"name":p.name,"mode":mode,"active":db.active().map_err(internal)?.id==project,"busy":state.busy.load(std::sync::atomic::Ordering::SeqCst),"messages":p.messages.iter().rev().take(80).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>(),"pending":p.pending,"spec":p.spec,"iterations":p.iterations,"operation":last,"previews":previews}),
    ))
}
pub(crate) fn record_preview(state: &AppState, p: &Project, stage: &Stage, raw: &Value) {
    let Some(items) = raw["content"].as_array() else {
        return;
    };
    for item in items {
        let (Some(mime), Some(data)) = (item["mimeType"].as_str(), item["data"].as_str()) else {
            continue;
        };
        if item["type"] != "image"
            || !matches!(mime, "image/png" | "image/jpeg")
            || data.len() > 4_000_000
        {
            continue;
        }
        if let Ok(db) = state.db.lock() {
            let _ = db.conn.execute(
                "INSERT INTO mobile_previews VALUES(?1,?2,?3,?4,?5,?6)",
                params![
                    id(),
                    p.id,
                    stage.key(),
                    p.iterations.last().map(|i| i.number),
                    now(),
                    format!("data:{mime};base64,{data}")
                ],
            );
            let _=db.conn.execute("DELETE FROM mobile_previews WHERE project=?1 AND rowid NOT IN(SELECT rowid FROM mobile_previews WHERE project=?1 ORDER BY rowid DESC LIMIT 12)",[&p.id]);
        }
        break;
    }
}
async fn preview(State(app): State<AppHandle>, h: HeaderMap, Json(req): Json<Stop>) -> Reply {
    let state = app.state::<AppState>();
    let db = state.db.lock().unwrap();
    let (pid, mode) = access(&db, &h)?;
    if mode.starts_with("pending-") {
        return Err(forbidden());
    }
    let image: String = db
        .conn
        .query_row(
            "SELECT image FROM mobile_previews WHERE project=?1 AND id=?2",
            params![pid, req.id],
            |r| r.get(0),
        )
        .optional()
        .map_err(internal)?
        .ok_or_else(|| conflict("预览已过期"))?;
    Ok(Json(json!({"image":image})))
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Operation {
    id: String,
    project: String,
    stage: Stage,
    text: Option<String>,
    choice: Option<String>,
    question_id: Option<String>,
}
fn receipt(
    db: &crate::store::Store,
    op: &Operation,
    payload: &str,
) -> Result<Option<Value>, Error> {
    let old = db
        .conn
        .query_row(
            "SELECT project,payload,status,detail FROM mobile_ops WHERE id=?1",
            [&op.id],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                ))
            },
        )
        .optional()
        .map_err(internal)?;
    match old {
        Some((p, body, status, detail)) if p == op.project && body == payload => {
            Ok(Some(json!({"id":op.id,"status":status,"detail":detail})))
        }
        Some(_) => Err(conflict("此操作编号已用于其他请求")),
        None => Ok(None),
    }
}
async fn operation(State(app): State<AppHandle>, h: HeaderMap, Json(op): Json<Operation>) -> Reply {
    if uuid::Uuid::parse_str(&op.id).is_err() {
        return Err(conflict("无效的操作编号"));
    }
    let (reply, rx) = oneshot::channel();
    // The worker owns the task even if its HTTP client disappears before receiving the receipt.
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let payload = fingerprint(&serde_json::to_string(&op).unwrap());
        let prepare = || {
            let db = state.db.lock().unwrap();
            let (pid, mode) = access(&db, &h)?;
            if mode != "control" || pid != op.project {
                return Err(forbidden());
            }
            if let Some(old) = receipt(&db, &op, &payload)? {
                return Ok((None, old));
            }
            let guard = state.begin().map_err(|e| conflict(&e))?;
            let mut p = db.active().map_err(internal)?;
            if p.id != pid {
                return Err(conflict("已切换项目，请在电脑上打开配对的项目"));
            }
            match (&p.pending, &op.choice, &op.text) {
                (Some(q), Some(c), None)
                    if Some(&q.call_id) == op.question_id.as_ref()
                        && q.stage == op.stage
                        && q.options.contains(c) => {}
                (None, None, Some(t))
                    if op.question_id.is_none() && !t.trim().is_empty() && t.len() <= 60000 => {}
                _ => return Err(conflict("问题或输入已变化，请刷新后操作")),
            }
            p.active_tab = p
                .tabs
                .iter()
                .find(|t| t.stage == op.stage)
                .ok_or_else(|| conflict("阶段不存在"))?
                .id
                .clone();
            db.save_project(&p).map_err(internal)?;
            db.conn
                .execute(
                    "INSERT INTO mobile_ops VALUES(?1,?2,?3,'running','')",
                    params![op.id, pid, payload],
                )
                .map_err(internal)?;
            Ok((Some(guard), json!({"id":op.id,"status":"running"})))
        };
        let (guard, answer) = match prepare() {
            Ok(v) => v,
            Err(e) => {
                let _ = reply.send(Err(e));
                return;
            }
        };
        let _ = reply.send(Ok(Json(answer)));
        let Some(guard) = guard else { return };
        agent::emit(
            Some(&app),
            &state.db.lock().unwrap().project(&op.project).unwrap(),
            "thinking",
        );
        let settings = state.db.lock().unwrap().settings();
        let result = match api::credential(&settings) {
            Ok(key) => agent::run(&state, Some(&app), op.text, op.choice, &key).await,
            Err(e) => Err(e),
        };
        let stopped = state.cancel.load(std::sync::atomic::Ordering::SeqCst);
        let (status, detail) = match result {
            Err(e) => ("failed", e),
            Ok(_) if stopped => (
                "stopped",
                "已停止后续操作，请核对进行中的 CAD 操作结果".into(),
            ),
            Ok(_) => ("done", String::new()),
        };
        {
            let db = state.db.lock().unwrap();
            let _ = db.conn.execute(
                "UPDATE mobile_ops SET status=?2,detail=?3 WHERE id=?1",
                params![op.id, status, detail],
            );
            if status == "failed" {
                if let Ok(mut p) = db.project(&op.project) {
                    p.messages.push(Message::new("error", detail, op.stage));
                    let _ = db.save_project(&p);
                }
            }
            drop(guard);
        }
        let _ = app.emit("agent-event", json!({"phase":"idle"}));
    });
    rx.await.map_err(internal)?
}
#[derive(Deserialize)]
struct Stop {
    id: String,
}
async fn stop(State(app): State<AppHandle>, h: HeaderMap, Json(req): Json<Stop>) -> Reply {
    let state = app.state::<AppState>();
    let db = state.db.lock().unwrap();
    let (pid, mode) = access(&db, &h)?;
    if mode != "control" {
        return Err(forbidden());
    }
    let running:bool=db.conn.query_row("SELECT EXISTS(SELECT 1 FROM mobile_ops WHERE project=?1 AND id=?2 AND status='running')",params![pid,req.id],|r|r.get(0)).map_err(internal)?;
    if running {
        state
            .cancel
            .store(true, std::sync::atomic::Ordering::SeqCst);
        state.cancel_notify.notify_waiters();
    }
    Ok(Json(json!({"requested":running})))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;

    #[test]
    fn reboot_preserves_terminal_receipts_and_marks_inflight_outcome_unknown() {
        let dir = tempfile::tempdir().unwrap();
        let op = Operation {
            id: id(), project: id(), stage: Stage::Requirements, text: Some("test".into()),
            choice: None, question_id: None,
        };
        let digest = fingerprint(&serde_json::to_string(&op).unwrap());
        {
            let db = Store::open(dir.path()).unwrap();
            initialize(&db).unwrap();
            db.conn.execute("INSERT INTO mobile_ops VALUES(?1,?2,?3,'running','')",params![op.id,op.project,digest]).unwrap();
        }
        let db = Store::open(dir.path()).unwrap();
        initialize(&db).unwrap();
        assert_eq!(receipt(&db,&op,&digest).unwrap().unwrap()["status"],"unknown");
        assert_eq!(receipt(&db,&op,"different").unwrap_err().0,StatusCode::CONFLICT);
        db.conn.execute("UPDATE mobile_ops SET status='done' WHERE id=?1",[&op.id]).unwrap();
        initialize(&db).unwrap();
        assert_eq!(receipt(&db,&op,&digest).unwrap().unwrap()["status"],"done");
    }
}
