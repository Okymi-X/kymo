mod assets;
pub mod auth;
pub mod db;
pub mod error;
pub mod routes;
pub mod state;

pub use routes::router;
pub use state::AppState;
