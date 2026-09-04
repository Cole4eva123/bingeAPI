//! Tauri command 层。
//!
//! 本模块是前端 `invoke(...)` 调用的入口。每个子模块按业务领域组织
//! command 函数，`mod.rs` 负责将它们统一 re-export，方便 `lib.rs`
//! 在 `generate_handler!` 宏中一次性注册。
//!
//! command 函数只负责以下三件事：
//! 1. 从 `tauri::State` 取出共享资源（如数据库连接池）；
//! 2. 调用 `db::repository` 或 `adaptor` 层的业务函数；
//! 3. 将结果或错误返回给前端。
//!
//! 不在 command 函数中编写 SQL 或直接发起 HTTP 请求。

mod channel;

// 将子模块中的 command 函数 re-export 到 `commands::` 命名空间，
// 这样 lib.rs 可以用 `commands::greet` 而不是 `commands::channel::greet`。
pub use channel::*;
