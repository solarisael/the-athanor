use protocol::LOOPBACK_HOST;

pub const MANAGED_DATABASE_USER: &str = "athanor";
pub const MANAGED_DATABASE_NAME: &str = "athanor";
pub const MANAGED_DATABASE_PORT: u16 = 5432;
pub const MANAGED_NATS_PORT: u16 = 4222;

pub fn managed_database_url(postgres_password: &str) -> String {
    format!(
        "postgresql://{MANAGED_DATABASE_USER}:{postgres_password}@{LOOPBACK_HOST}:{MANAGED_DATABASE_PORT}/{MANAGED_DATABASE_NAME}"
    )
}
