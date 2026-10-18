use axum::{
    extract::{Extension, Path, Query},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, net::SocketAddr};
use tokio::net::TcpListener;

#[derive(Serialize)]
struct ApiDevice {
    id: String,
    uuid: String,
    status: i64,
    note: Option<String>,
}

#[derive(Serialize)]
struct ApiDeviceList {
    items: Vec<ApiDevice>,
    page: u32,
    limit: u32,
    total: u64,
}

#[derive(Deserialize)]
struct ApiDeviceFilter {
    status: Option<i64>,
    page: Option<u32>,
    limit: Option<u32>,
}

#[derive(Deserialize)]
struct ApiDevicePatch {
    status: Option<i64>,
    note: Option<Option<String>>,
}

async fn health() -> &'static str {
    "ok"
}

async fn get_devices(
	Extension(pm): Extension<crate::peer::PeerMap>,
    Query(filter): Query<ApiDeviceFilter>,
) -> Json<ApiDeviceList> {

    let page = filter.page.unwrap_or(1);
	let limit = filter.limit.unwrap_or(50);

	let total = pm
		.api_count_peers(filter.status)
		.await
		.unwrap_or(0);

	let peers = pm
		.api_list_peers(filter.status, page, limit)
		.await
		.unwrap_or_default();

	let items = peers
		.into_iter()
		.map(|peer| ApiDevice {
			id: peer.id,			
			uuid: String::from_utf8_lossy(&peer.uuid).into_owned(),
			status: peer.status.unwrap_or(crate::database::STATUS_WAITING),
			note: peer.note,
		})
		.collect();

	Json(ApiDeviceList {
		items,
		page,
		limit,
		total,
	})
}

async fn get_device(
	Extension(pm): Extension<crate::peer::PeerMap>,
    Path(id): Path<String>,
) -> Result<Json<ApiDevice>, axum::http::StatusCode> {

	let peer = pm
		.api_get_peer(&id)
        .await
        .map_err(|err| {
            log::error!("get_peer failed for {}: {}", id, err);
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        })?;

    match peer {
        Some(peer) => Ok(Json(ApiDevice {
            id: peer.id,
            uuid: String::from_utf8_lossy(&peer.uuid).into_owned(),
            status: peer.status.unwrap_or(crate::database::STATUS_WAITING),
            note: peer.note,
        })),
        None => Err(axum::http::StatusCode::NOT_FOUND),
    }
}

async fn patch_device(
	Extension(pm): Extension<crate::peer::PeerMap>,
    Path(id): Path<String>,
    Json(patch): Json<ApiDevicePatch>,
) -> Result<Json<HashMap<&'static str, &'static str>>, axum::http::StatusCode> {

    if let Some(status) = patch.status {
		if !matches!(
			status,
			crate::database::STATUS_WAITING
				| crate::database::STATUS_APPROVED
				| crate::database::STATUS_BLOCKED
		) {
			return Err(axum::http::StatusCode::BAD_REQUEST);
		}

		if let Err(err) = pm.api_set_status(&id, status).await {
			log::error!("set_status failed for {}: {}", id, err);
			return Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR);
		}
	}

    if let Some(note) = patch.note {
		if let Err(err) = pm.api_set_note(&id, note.as_deref()).await {
			log::error!("set_note failed for {}: {}", id, err);
			return Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR);
		}
	}

    Ok(Json(HashMap::from([
		("status", "ok"),
	])))
}
		
pub async fn start_api(pm: crate::peer::PeerMap) {
    let app = Router::new()
        .route("/health", get(health))
        .route("/devices", get(get_devices))
        .route("/devices/:id", get(get_device).patch(patch_device))
        .layer(Extension(pm));

    let addr = SocketAddr::from(([0, 0, 0, 0], 31120));

    let listener = match TcpListener::bind(addr).await {
        Ok(listener) => listener,
        Err(e) => {
            log::error!("API bind failed: {}", e);
            return;
        }
    };

    let result = axum::Server::from_tcp(listener.into_std().unwrap())
        .unwrap()
        .serve(app.into_make_service())
        .await;

    match result {
        Ok(_) => log::warn!("API exited normally"),
        Err(e) => log::error!("API exited: {}", e),
    }
}