# Whole-branch Rival Codex review

Reviewed committed `e3d57b6`; generated mismatch excluded while hosted generation ran. Both findings were independently fixed in `1f0d463` before the review returned. Hosted regression acceptance remains pending.

```text

═══ RIVAL REVIEW ═══

Model: codex (gpt-6-astra)
Scope: master...HEAD in /Users/kass/dev/lobocode-cloud-only. Review the cloud-only Mac app change and app-only release workflow against plans/2026-10-02-cloud-only-app/spec.md. Focus on correctness: no reachable local execution, cloud-only operations and ownership races, preserved CLI local config/state, Q6 defaults, standalone release validation. Generated TS/JSON regeneration is pending hosted macOS and known stale; do not report those temporary mismatches. No builds/tests or inference on this Mac. Read-only review, do not edit files. Report material bugs with file/line and mechanism.

Summary: Found two cloud setup defects in committed revision e3d57b6. No builds or tests were run; uncommitted edits and generated-file mismatches were excluded.

1. [high] Saved provider can remain selected after its key is removed — app/src-tauri/src/backend.rs:152
   cloud_config preserves any RunPod/Vast preference without checking whether that provider has credentials. config() exposes that preference directly as default_provider. With only the other provider configured, cloud_ready remains true, but discovery and Start fail. Both provider pickers disappear when only one provider is available, preventing correction through the UI.
   Scenario: LOBO_PROVIDER=runpod with both provider keys configured; remove the RunPod key in Settings → the app keeps selecting RunPod, hides the provider picker, and cannot start using the remaining Vast key.
   Fix: Normalize the effective provider against cfg.providers(), falling back to an available cloud provider without changing the saved CLI configuration.
   (bug, confidence 10)

2. [med] Hidden local settings still block cloud startup — app/src-tauri/src/backend.rs:329
   The effective configuration still contains LOBO_LOCAL_PORT when passed to resolve_up. Shared defaults validation checks that local-only value and rejects cloud startup if it is invalid. The app now hides this setting and rejects its writes, so cloud setup cannot repair the failure.
   Scenario: An existing config contains LOBO_LOCAL_PORT=70000 alongside valid API and RunPod credentials → cloud_ready is true, but every cloud Start fails with a LOBO_LOCAL_PORT validation error.
   Fix: Exclude local-only values from the app's in-memory admission configuration. Preserve the original CLI values on disk and retain cloud-port validation.
   (bug, confidence 9)

Findings: 2 total — 0 crit, 1 high, 1 med, 0 low
Log: /Users/kass/.rival/sessions/77a47d5a-c4bb-4f7e-8fa2-7f600fb08a93.log

```
