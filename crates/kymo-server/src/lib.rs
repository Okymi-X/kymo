mod assets;
pub mod auth;
pub mod calendar;
pub mod db;
pub mod demo;
pub mod error;
pub mod events;
pub mod routes;
pub mod settings;
pub mod state;
pub mod text;
pub mod tokens;

pub use routes::router;
pub use state::AppState;
