mod actions;
mod diff;
mod gitlab;
mod info;
mod review;
mod tours;
mod tree;

use std::sync::Arc;

use axum::Router;

use crate::server::AppState;

pub fn api_routes() -> Router<Arc<AppState>> {
    Router::new()
        .merge(diff::routes())
        .merge(info::routes())
        .merge(review::routes())
        .merge(tours::routes())
        .merge(actions::routes())
        .merge(tree::routes())
        .merge(gitlab::routes())
}
