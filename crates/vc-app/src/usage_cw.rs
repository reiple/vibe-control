// Account-wide "today" token usage, read from Amazon CloudWatch.
//
// The in-app Bedrock bearer token (AWS_BEARER_TOKEN_BEDROCK) is invoke-only — it
// cannot read usage APIs. To show tokens used by the whole AWS account today
// (not just this app's console calls), we query the automatic `AWS/Bedrock`
// CloudWatch metrics — InputTokenCount / OutputTokenCount / Invocations — summed
// across every model in the configured region since local midnight.
//
// This needs STANDARD AWS credentials (env `AWS_ACCESS_KEY_ID`/`AWS_SECRET_ACCESS_KEY`,
// an `~/.aws` profile, SSO, etc.) with `cloudwatch:GetMetricData`. When none are
// available the query errors and the caller falls back to the local tally.
//
// Only integer token counts cross back — never a credential, prompt, or response.

use std::time::Duration;

use aws_sdk_cloudwatch::primitives::DateTime;
use aws_sdk_cloudwatch::types::{MetricDataQuery, ScanBy};

/// Account-wide token totals for a time window (summed across all Bedrock models).
#[derive(Debug, Clone, Copy, Default)]
pub struct AccountUsage {
    pub input: u64,
    pub output: u64,
    pub calls: u64,
}

/// Sum `AWS/Bedrock` token metrics from `since_epoch` (local midnight, seconds)
/// to `now_epoch` for the given region. Uses a Metrics-Insights SEARCH so it
/// aggregates across every model id without enumerating them. Bounded by an
/// overall timeout so a missing-credential / IMDS probe can't stall the UI.
pub async fn fetch_today(
    region: Option<String>,
    since_epoch: i64,
    now_epoch: i64,
) -> Result<AccountUsage, String> {
    match tokio::time::timeout(
        Duration::from_secs(8),
        fetch_inner(region, since_epoch, now_epoch),
    )
    .await
    {
        Ok(res) => res,
        Err(_) => Err("cloudwatch: timed out".to_string()),
    }
}

async fn fetch_inner(
    region: Option<String>,
    since_epoch: i64,
    now_epoch: i64,
) -> Result<AccountUsage, String> {
    let mut loader = aws_config::defaults(aws_config::BehaviorVersion::latest());
    if let Some(r) = region.filter(|r| !r.is_empty()) {
        loader = loader.region(aws_config::Region::new(r));
    }
    let conf = loader.load().await;
    let client = aws_sdk_cloudwatch::Client::new(&conf);

    // One 5-minute-bucket SEARCH per metric, wrapped in SUM() to collapse the
    // per-model series into a single account-wide series. 300s buckets keep the
    // start boundary tight for any timezone offset; we sum the buckets below.
    let q = |id: &str, metric: &str| -> MetricDataQuery {
        MetricDataQuery::builder()
            .id(id)
            .expression(format!(
                "SUM(SEARCH('{{AWS/Bedrock}} MetricName=\"{metric}\"', 'Sum', 300))"
            ))
            .return_data(true)
            .build()
    };

    let resp = client
        .get_metric_data()
        .start_time(DateTime::from_secs(since_epoch))
        .end_time(DateTime::from_secs(now_epoch))
        .metric_data_queries(q("intok", "InputTokenCount"))
        .metric_data_queries(q("outtok", "OutputTokenCount"))
        .metric_data_queries(q("calls", "Invocations"))
        .scan_by(ScanBy::TimestampDescending)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let mut usage = AccountUsage::default();
    for r in resp.metric_data_results() {
        let sum: f64 = r.values().iter().copied().filter(|v| v.is_finite()).sum();
        let n = if sum > 0.0 { sum.round() as u64 } else { 0 };
        match r.id() {
            Some("intok") => usage.input = n,
            Some("outtok") => usage.output = n,
            Some("calls") => usage.calls = n,
            _ => {}
        }
    }
    Ok(usage)
}
