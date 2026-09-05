use async_trait::async_trait;
use serde_json::{json, Value};

use super::*;
use crate::adapter::{ProxyRequest, ProxyResponse};
use crate::db;
use crate::error::CommandError;

fn input() -> CreateChannelInput {
    serde_json::from_value(json!({
        "name": "Test channel",
        "type": "deepseek",
        "base_url": "https://example.com/v1",
        "api_key": "private-test-key",
        "models": ["alias"],
        "config": {"project": "demo"},
        "model_mapping": {"alias": "upstream"}
    }))
    .unwrap()
}

#[test]
fn channel_round_trip_preserves_partial_updates_and_hides_key() {
    tauri::async_runtime::block_on(async {
        let pool = db::test_pool().await;
        let channel = create_channel(&pool, input()).await.unwrap();
        let id = channel.info.id.clone();
        assert_eq!(channel.info.weight, 1);
        assert_eq!(channel.info.status, 1);
        assert_eq!(channel.info.priority, 0);
        let json = serde_json::to_value(&channel.info).unwrap();
        assert_eq!(json["models"], json!(["alias"]));
        assert_eq!(json["model_mapping"], json!({"alias": "upstream"}));
        assert_eq!(json["has_api_key"], true);
        assert!(json.get("api_key").is_none());

        let updated = update_channel(
            &pool,
            UpdateChannelInput {
                id: id.clone(),
                name: Some("Renamed".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(updated.info.name, "Renamed");
        assert_eq!(updated.api_key, channel.api_key);
        assert_eq!(updated.info.models, channel.info.models);
        assert_eq!(updated.info.config, channel.info.config);
        assert_eq!(updated.info.model_mapping, channel.info.model_mapping);

        let cleared = update_channel(
            &pool,
            UpdateChannelInput {
                id: id.clone(),
                models: Some(vec![]),
                config: Some(Default::default()),
                model_mapping: Some(Default::default()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert!(cleared.info.models.is_empty());
        assert!(cleared.info.config.is_empty());
        assert!(cleared.info.model_mapping.is_empty());
        assert_eq!(repository::count_channels(&pool).await.unwrap(), 1);
        assert_eq!(repository::list_channels(&pool).await.unwrap().len(), 1);
        repository::delete_channel(&pool, &id).await.unwrap();
        assert!(repository::find_channel(&pool, &id)
            .await
            .unwrap()
            .is_none());
        assert!(matches!(
            repository::delete_channel(&pool, &id).await,
            Err(AppError::NotFound(_))
        ));
        assert!(matches!(
            update_channel(
                &pool,
                UpdateChannelInput {
                    id,
                    ..Default::default()
                }
            )
            .await,
            Err(AppError::NotFound(_))
        ));
    });
}

#[test]
fn invalid_inputs_are_rejected_before_writing() {
    tauri::async_runtime::block_on(async {
        let pool = db::test_pool().await;
        for (field, value) in [
            ("type", json!("unknown")),
            ("name", json!(" ")),
            ("base_url", json!("file:///tmp/test")),
            ("api_key", json!("")),
            ("weight", json!(0)),
            ("models", json!([""])),
        ] {
            let mut value_input = json!({
                "name": "Valid", "type": "openai", "base_url": "https://example.com/v1",
                "api_key": "test", "models": []
            });
            value_input[field] = value;
            let result = create_channel(&pool, serde_json::from_value(value_input).unwrap()).await;
            assert!(
                matches!(result, Err(AppError::InvalidInput(_))),
                "{field} should fail"
            );
        }
        assert_eq!(repository::count_channels(&pool).await.unwrap(), 0);
        let channel = create_channel(&pool, input()).await.unwrap();
        let result = update_channel(
            &pool,
            UpdateChannelInput {
                id: channel.info.id.clone(),
                status: Some(2),
                name: Some("Should not save".into()),
                ..Default::default()
            },
        )
        .await;
        assert!(matches!(result, Err(AppError::InvalidInput(_))));
        assert_eq!(
            repository::find_channel(&pool, &channel.info.id)
                .await
                .unwrap()
                .unwrap()
                .info
                .name,
            "Test channel"
        );
        for patch in [
            json!({"model_mapping": {"alias": 123}}),
            json!({"config": []}),
        ] {
            let mut value = json!({"id": channel.info.id});
            value
                .as_object_mut()
                .unwrap()
                .extend(patch.as_object().unwrap().clone());
            assert!(serde_json::from_value::<UpdateChannelInput>(value).is_err());
        }
    });
}

#[test]
fn invalid_stored_json_is_reported_with_field_and_error_code() {
    tauri::async_runtime::block_on(async {
        let pool = db::test_pool().await;
        let channel = create_channel(&pool, input()).await.unwrap();
        sqlx::query("UPDATE channels SET model_mapping = ? WHERE id = ?")
            .bind(r#"{"alias":123}"#)
            .bind(&channel.info.id)
            .execute(&pool)
            .await
            .unwrap();
        let error = repository::find_channel(&pool, &channel.info.id)
            .await
            .err()
            .unwrap();
        assert!(matches!(
            error,
            AppError::InvalidStoredData {
                field: "model_mapping",
                ..
            }
        ));
        let response = serde_json::to_value(CommandError::from(error)).unwrap();
        assert_eq!(response["code"], "invalid_stored_data");
        assert!(response["message"]
            .as_str()
            .unwrap()
            .contains("model_mapping"));
    });
}

struct FakeAdapter {
    success: bool,
    internal_error: bool,
}

#[async_trait]
impl Adapter for FakeAdapter {
    async fn test(&self, config: &ChannelConfig) -> anyhow::Result<TestResult> {
        assert_eq!(config.api_key, "private-test-key");
        assert_eq!(config.model_mapping["alias"], "upstream");
        assert_eq!(config.extra["project"], "demo");
        if self.internal_error {
            anyhow::bail!("test failed internally");
        }
        Ok(TestResult {
            success: self.success,
            message: "test".into(),
            latency_ms: 1,
        })
    }
    async fn forward(&self, _: &ProxyRequest, _: &ChannelConfig) -> anyhow::Result<ProxyResponse> {
        unreachable!()
    }
    async fn forward_stream(
        &self,
        _: &ProxyRequest,
        _: &ChannelConfig,
    ) -> anyhow::Result<reqwest::Response> {
        unreachable!()
    }
}

#[test]
fn test_outcomes_are_persisted_including_internal_failure() {
    tauri::async_runtime::block_on(async {
        let pool = db::test_pool().await;
        let channel = create_channel(&pool, input()).await.unwrap();
        for (success, internal_error) in
            [(true, false), (false, false), (true, false), (false, true)]
        {
            let result = run_test(
                &pool,
                channel.clone(),
                &FakeAdapter {
                    success,
                    internal_error,
                },
            )
            .await;
            assert_eq!(result.is_err(), internal_error);
            let saved = repository::find_channel(&pool, &channel.info.id)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(saved.info.last_test_ok, Some(i64::from(success)));
            assert!(saved.info.last_test_at.is_some());
        }
        let missing = test_channel(&pool, &reqwest::Client::new(), "missing").await;
        assert!(matches!(missing, Err(AppError::NotFound(_))));
    });
}

#[test]
fn command_errors_have_stable_codes() {
    let error = CommandError::from(AppError::NotFound("missing".into()));
    let value: Value = serde_json::to_value(error).unwrap();
    assert_eq!(
        value,
        json!({"code": "not_found", "message": "channel not found: missing"})
    );
}
