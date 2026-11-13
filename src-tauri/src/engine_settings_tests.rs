use super::*;
use crate::settings::Settings;

async fn ready(rpc: &Rpc) {
    for _ in 0..50 {
        if rpc.call("aria2.getVersion", vec![]).await.is_ok() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("aria2 was not ready");
}

async fn option(rpc: &Rpc, key: &str) -> String {
    let value = rpc.call("aria2.getGlobalOption", vec![]).await.unwrap();
    value[key].as_str().unwrap_or_default().to_string()
}

// Settings are written straight into aria2 so the limit is enforced from the
// first request, and later edits must reach the running engine without a restart.
#[tokio::test]
async fn global_limits_apply_at_start_and_change_on_the_running_engine() {
    let initial = Settings {
        max_overall_download_limit: 1500,
        max_concurrent_downloads: 6,
        ..Settings::default()
    };
    let engine = Engine::start_with(&initial).unwrap();
    ready(&engine.rpc).await;
    // aria2 把 "1500K" 归一化为字节数返回。
    assert_eq!(
        option(&engine.rpc, "max-overall-download-limit").await,
        "1536000"
    );
    assert_eq!(option(&engine.rpc, "max-concurrent-downloads").await, "6");

    let changed = Settings {
        max_overall_download_limit: 0,
        max_concurrent_downloads: 2,
        ..Settings::default()
    };
    engine
        .rpc
        .change_global_options(&changed.aria2_options())
        .await
        .unwrap();
    assert_eq!(option(&engine.rpc, "max-overall-download-limit").await, "0");
    assert_eq!(option(&engine.rpc, "max-concurrent-downloads").await, "2");
}
