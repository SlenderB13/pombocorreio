use crate::{
    models::Offer,
    state::{emit_change, CoreState, PendingOffer},
    storage::{publish_received, safe_destination},
};
use serde::Serialize;
use std::{
    fs::{self, File},
    net::TcpListener,
    thread,
};
use tauri::AppHandle;
use tiny_http::{Header, Method, Response, Server, StatusCode};

fn write_json<T: Serialize>(status: u16, value: &T) -> Response<std::io::Cursor<Vec<u8>>> {
    let body = serde_json::to_vec(value).unwrap_or_default();
    Response::from_data(body)
        .with_status_code(StatusCode(status))
        .with_header(Header::from_bytes("content-type", "application/json").expect("valid header"))
}

pub(crate) fn start_http_server(
    listener: TcpListener,
    state: CoreState,
    app: AppHandle,
) -> Result<(), String> {
    let server = Server::from_listener(listener, None).map_err(|error| error.to_string())?;
    thread::spawn(move || {
        for request in server.incoming_requests() {
            let method = request.method().clone();
            let path = request
                .url()
                .trim_matches('/')
                .split('/')
                .map(str::to_owned)
                .collect::<Vec<_>>();

            if method == Method::Post && path.as_slice() == ["v1", "offers"] {
                receive_offer(request, &state, &app);
            } else if method == Method::Get && path.len() == 3 && path[..2] == ["v1", "offers"] {
                report_offer_status(request, &path[2], &state);
            } else if method == Method::Post && path.len() == 4 && path[..2] == ["v1", "files"] {
                receive_file(request, &path[2], &path[3], &state, &app);
            } else {
                let _ =
                    request.respond(write_json(404, &serde_json::json!({"error": "not found"})));
            }
        }
    });
    Ok(())
}

fn receive_offer(mut request: tiny_http::Request, state: &CoreState, app: &AppHandle) {
    let mut body = String::new();
    let result = request
        .as_reader()
        .read_to_string(&mut body)
        .map_err(|error| error.to_string())
        .and_then(|_| serde_json::from_str::<Offer>(&body).map_err(|error| error.to_string()));

    match result {
        Ok(offer) => {
            let trusted = {
                let inner = state.0.lock().expect("state lock");
                inner.settings.trusted_devices.contains(&offer.sender_id)
            };
            state.0.lock().expect("state lock").offers.insert(
                offer.id.clone(),
                PendingOffer {
                    offer,
                    accepted: trusted.then_some(true),
                },
            );
            emit_change(app);
            let status = if trusted { "accepted" } else { "pending" };
            let response = write_json(202, &serde_json::json!({"status": status}));
            let _ = request.respond(response);
        }
        Err(error) => {
            let _ = request.respond(write_json(400, &serde_json::json!({"error": error})));
        }
    }
}

fn report_offer_status(request: tiny_http::Request, offer_id: &str, state: &CoreState) {
    let status = state
        .0
        .lock()
        .expect("state lock")
        .offers
        .get(offer_id)
        .map(|pending| match pending.accepted {
            Some(true) => "accepted",
            Some(false) => "declined",
            None => "pending",
        })
        .unwrap_or("missing");
    let _ = request.respond(write_json(200, &serde_json::json!({"status": status})));
}

fn receive_file(
    mut request: tiny_http::Request,
    transfer_id: &str,
    requested_index: &str,
    state: &CoreState,
    app: &AppHandle,
) {
    let target = requested_index.parse::<usize>().ok().and_then(|index| {
        let inner = state.0.lock().expect("state lock");
        let pending = inner.offers.get(transfer_id)?;
        if pending.accepted != Some(true) {
            return None;
        }
        let file = pending.offer.files.get(index)?;
        Some(safe_destination(&inner.settings.inbox, &file.name))
    });

    let Some(target) = target else {
        let response = write_json(
            403,
            &serde_json::json!({"error": "transfer was not accepted"}),
        );
        let _ = request.respond(response);
        return;
    };

    match File::create(&target).and_then(|mut file| std::io::copy(request.as_reader(), &mut file)) {
        Ok(_) => finish_received_file(request, target, app),
        Err(error) => {
            let response = write_json(500, &serde_json::json!({"error": error.to_string()}));
            let _ = request.respond(response);
        }
    }
}

fn finish_received_file(request: tiny_http::Request, target: std::path::PathBuf, app: &AppHandle) {
    let name = target
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("received-file")
        .to_string();

    match publish_received(app, &target, &name) {
        Ok(saved) => {
            emit_change(app);
            let _ = request.respond(write_json(201, &serde_json::json!({"saved": saved})));
        }
        Err(error) => {
            let _ = fs::remove_file(&target);
            let _ = request.respond(write_json(500, &serde_json::json!({"error": error})));
        }
    }
}
