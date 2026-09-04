//! SQLite 数据库模块。
//!
//! 本模块 **只** 负责数据库相关的职责：
//! 1. 计算数据库文件的位置；
//! 2. 创建 SQLx SQLite 连接池；
//! 3. 应用启动时执行 `init.sql`，确保表和索引已经存在。
//!
//! 业务 command 不需要重复打开数据库，只需要通过
//! `tauri::State<'_, Database>` 取得这里创建好的连接池即可。
//!
//! 上游 API 适配器（OpenAI、DeepSeek 等）已移至独立的 `adaptor` 模块，
//! 与数据库层解耦。

// 对外暴露数据模型和 repository，供 command 层使用。
pub mod models;
pub mod repository;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use tauri::{AppHandle, Manager};

/// 应用级数据库状态。
///
/// `SqlitePool` 不是一条单独的 SQLite 连接，而是一个连接池：
/// SQLx 会根据并发情况复用池中的连接。将它放进 Tauri State 后，
/// 所有 command 都可以共享同一个连接池，不需要每次请求都重新打开
/// `.db` 文件。
pub struct Database {
    pub pool: SqlitePool,
}

impl Database {
    /// 创建并初始化应用数据库。
    ///
    /// `app` 用来获取 Tauri 为当前应用分配的数据目录。数据库文件放在
    /// 这个目录下，而不是项目目录或可执行文件目录，原因是：
    /// - 项目目录在打包后通常不存在；
    /// - 可执行文件目录可能没有写权限；
    /// - 应用数据目录会随操作系统和应用标识自动选择合适的位置。
    ///
    /// 返回 `Result` 而不是直接 `unwrap`，这样启动失败时可以把具体原因
    /// 传回 Tauri 的 setup 流程，而不是产生不容易定位的 panic。
    pub async fn new(app: &AppHandle) -> Result<Self, String> {
        // 获取应用专属的数据目录。
        // 例如 macOS 通常位于 ~/Library/Application Support/<bundle-id>。
        let app_data_dir = app
            .path()
            .app_data_dir()
            .map_err(|error| format!("failed to get app data dir: {error}"))?;

        // 第一次启动时目录可能还不存在，因此需要递归创建目录。
        // 目录已经存在时，create_dir_all 不会报错。
        std::fs::create_dir_all(&app_data_dir)
            .map_err(|error| format!("failed to create app data dir: {error}"))?;

        // join 会根据当前操作系统生成正确的路径分隔符。
        // SQLite 文件不存在时，后面的 create_if_missing(true) 会创建它。
        let db_path = app_data_dir.join("bingeapi.db");

        // 使用 SqliteConnectOptions 传入 Path，而不是手动拼接
        // `sqlite://...` URL。这样可以正确处理路径中的空格、中文和其他
        // 需要 URL 编码的字符。
        let options = SqliteConnectOptions::new()
            .filename(&db_path)
            .create_if_missing(true);

        // 创建连接池。
        // max_connections(5) 表示最多同时保留 5 条 SQLite 连接；对于桌面端
        // 管理面板已经足够。后续所有 SQL 查询都通过这个 pool 执行。
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await
            .map_err(|error| format!("failed to connect to database: {error}"))?;

        // include_str! 在编译时把仓库根目录的 init.sql 嵌入程序。
        //
        // 当前文件位于 src-tauri/src/db/mod.rs：
        //   ..       -> src-tauri/src
        //   ../..    -> src-tauri
        //   ../../.. -> 仓库根目录
        // 因此 ../../../init.sql 正好指向根目录的初始化脚本。
        //
        // raw_sql 可以执行一个字符串中的多条 SQL 语句，适合执行包含
        // CREATE TABLE 和 CREATE INDEX 的完整初始化脚本。
        sqlx::raw_sql(include_str!("../../../init.sql"))
            .execute(&pool)
            .await
            .map_err(|error| format!("failed to initialize database: {error}"))?;

        // 只有连接、建库或执行初始化脚本都成功后，才把 Database 返回给
        // Tauri。这样后续 command 拿到的 State 一定是可用的。
        Ok(Self { pool })
    }
}
