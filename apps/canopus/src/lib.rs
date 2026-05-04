pub mod adapters;
pub mod cli;
pub mod core;
pub mod ports;

#[cfg(test)]
pub(crate) mod test_support {
    use std::sync::{LazyLock, Mutex};

    pub(crate) static ENV_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));
}
