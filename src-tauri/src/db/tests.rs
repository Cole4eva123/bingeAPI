use super::*;
use crate::db::api_key::get_api_key_by_key;
use crate::db::request_log::{search_logs, LogFilter, Pagination};

#[test]
fn adopts_legacy_database_and_reopens_without_losing_records() {
    tauri::async_runtime::block_on(async {
        let path =
            std::env::temp_dir().join(format!("bingeapi-migration-{}.db", uuid::Uuid::new_v4()));
        let options = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true);
        let legacy = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .unwrap();
        // 独立保存旧版 schema，防止修改迁移时把测试基线一并改变。
        sqlx::raw_sql(include_str!("fixtures/legacy.sql"))
            .execute(&legacy)
            .await
            .unwrap();
        sqlx::query(r#"INSERT INTO channels
            (id, name, type, base_url, api_key, models, created_at, updated_at)
            VALUES ('existing', 'Existing', 'openai', 'https://example.com/v1', 'old-key', '["m"]', 'before', 'before')"#)
            .execute(&legacy).await.unwrap();
        sqlx::query(
            r#"INSERT INTO api_keys (id, name, key, created_at, updated_at)
            VALUES ('key-id', 'Old key', 'client-key', 'before', 'before')"#,
        )
        .execute(&legacy)
        .await
        .unwrap();
        sqlx::query(
            r#"INSERT INTO request_logs (id, model, mode, status_code, created_at)
            VALUES ('log-id', 'm', 'chat', 200, 'before')"#,
        )
        .execute(&legacy)
        .await
        .unwrap();
        legacy.close().await;

        for _ in 0..2 {
            let db = Database::open(&path).await.unwrap();
            let channel = channel::find_channel(&db.pool, "existing")
                .await
                .unwrap()
                .unwrap();
            assert_eq!(channel.api_key, "old-key");
            assert_eq!(channel.info.models, vec!["m"]);
            assert_eq!(channel.info.created_at, "before");
            assert_eq!(
                get_api_key_by_key(&db.pool, "client-key")
                    .await
                    .unwrap()
                    .unwrap()
                    .id,
                "key-id"
            );
            let logs = search_logs(&db.pool, &LogFilter::default(), Pagination::default())
                .await
                .unwrap();
            assert_eq!(logs.len(), 1);
            assert_eq!(logs[0].id, "log-id");
            let versions: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations WHERE success = 1")
                    .fetch_one(&db.pool)
                    .await
                    .unwrap();
            assert_eq!(versions, 1);
            db.pool.close().await;
        }
        std::fs::remove_file(path).unwrap();
    });
}

#[test]
fn log_filters_and_pagination_use_real_schema_fields() {
    tauri::async_runtime::block_on(async {
        let pool = test_pool().await;
        for (id, name, model, date) in [
            ("a", "Alpha", "m1", "2026-09-01T00:00:00.000Z"),
            ("b", "Beta", "m2", "2026-09-02T00:00:00.000Z"),
            ("c", "Alpha", "m1", "2026-09-03T00:00:00.000Z"),
        ] {
            sqlx::query("INSERT INTO request_logs (id, channel_name, model, mode, status_code, created_at) VALUES (?, ?, ?, 'chat', 200, ?)")
                .bind(id).bind(name).bind(model).bind(date).execute(&pool).await.unwrap();
        }
        let filter = LogFilter {
            keyword: Some(" Alpha ".into()),
            channel_name: Some("Alpha".into()),
            model: Some("m1".into()),
            date_from: Some("2026-09-02T00:00:00.000Z".into()),
            date_to: Some("2026-09-04T00:00:00.000Z".into()),
        };
        let logs = search_logs(&pool, &filter, Pagination::default())
            .await
            .unwrap();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].id, "c");
        let second = search_logs(
            &pool,
            &LogFilter::default(),
            Pagination {
                limit: 1,
                offset: 1,
            },
        )
        .await
        .unwrap();
        assert_eq!(second[0].id, "b");
        let clamped = search_logs(
            &pool,
            &LogFilter::default(),
            Pagination {
                limit: 0,
                offset: -1,
            },
        )
        .await
        .unwrap();
        assert_eq!(clamped.len(), 1);
        assert_eq!(clamped[0].id, "c");
        let injection = LogFilter {
            keyword: Some("' OR 1=1 --".into()),
            ..Default::default()
        };
        assert!(search_logs(&pool, &injection, Pagination::default())
            .await
            .unwrap()
            .is_empty());
        let value = serde_json::to_value(&logs[0]).unwrap();
        assert!(value.get("risk_level").is_none());
        assert!(value.get("security_action").is_none());
    });
}
