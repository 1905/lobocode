#!/usr/bin/env python3
import unittest

import spec_bench as sb


def row(config, workload, tok_s, rep=1, mode="greedy", label="sweep", sha="a", **kw):
    r = {"kind": "row", "label": label, "config": config, "workload": workload, "mode": mode, "rep": rep,
         "predicted_per_second": tok_s, "prompt_per_second": 1000.0, "prompt_ms": 500.0,
         "predicted_n": 100, "text_sha256": sha}
    r.update(kw)
    return r


def sweep(configs):
    """configs: {name: {workload: tok_s}}; every config writes two greedy reps."""
    return [row(c, w, v, rep=rep) for c, ws in configs.items() for w, v in ws.items() for rep in (1, 2)]


class Pure(unittest.TestCase):
    def test_median(self):
        self.assertEqual(sb.median([3, 1, 2]), 2)
        self.assertEqual(sb.median([1, 2, 3, 4]), 2.5)
        self.assertEqual(sb.median([7]), 7)
        self.assertEqual(sb.median([None, 5]), 5)
        with self.assertRaises(ValueError):
            sb.median([])

    def test_summarize_drops_warmup_and_errors(self):
        rows = [row("off", "W1", 999, rep=0, warmup=True), row("off", "W1", 50),
                row("off", "W1", 60, rep=2), row("off", "W1", None, rep=3, error="timeout")]
        s = sb.summarize(rows)[("sweep", "off", "W1", "greedy")]
        self.assertEqual((s["tok_s"], s["n"], s["cold_ttft_s"]), (55, 2, 0.5))

    def test_summarize_accept_rate(self):
        rows = [row("mtp2", "W1", 70, draft_n=10, draft_n_accepted=7),
                row("mtp2", "W1", 70, rep=2, draft_n=10, draft_n_accepted=9)]
        self.assertAlmostEqual(sb.summarize(rows)[("sweep", "mtp2", "W1", "greedy")]["accept_rate"], 0.8)

    def test_gate_picks_best_chat_gain(self):
        rows = sweep({"off": {"W1": 50, "W2": 50, "W3": 50},
                      "mtp2": {"W1": 65, "W2": 60, "W3": 70},
                      "ngram": {"W1": 56, "W2": 56, "W3": 150}})
        s = sb.summarize(rows)
        winner, reasons = sb.gate(s, sb.greedy_matches(rows, "sweep"), "sweep")
        self.assertEqual(winner, "mtp2")
        self.assertEqual(reasons, [])

    def test_gate_rejects_small_chat_gain(self):
        rows = sweep({"off": {"W1": 50, "W2": 50}, "ngram": {"W1": 52, "W2": 53}})
        s = sb.summarize(rows)
        winner, reasons = sb.gate(s, sb.greedy_matches(rows, "sweep"), "sweep")
        self.assertIsNone(winner)
        self.assertIn("chat gain", reasons[0])

    def test_gate_rejects_slower_workload(self):
        rows = sweep({"off": {"W1": 50, "W2": 50, "W4": 50}, "mtp3": {"W1": 70, "W2": 70, "W4": 45}})
        s = sb.summarize(rows)
        winner, reasons = sb.gate(s, sb.greedy_matches(rows, "sweep"), "sweep")
        self.assertIsNone(winner)
        self.assertIn("slower", reasons[0])

    def test_gate_rejects_greedy_mismatch(self):
        rows = sweep({"off": {"W1": 50, "W2": 50}})
        rows += [row("mtp2", w, 70, rep=rep, sha="b") for w in ("W1", "W2") for rep in (1, 2)]
        s = sb.summarize(rows)
        matches = sb.greedy_matches(rows, "sweep")
        self.assertEqual(matches["mtp2"], (0, 4))
        winner, reasons = sb.gate(s, matches, "sweep")
        self.assertIsNone(winner)
        self.assertIn("greedy match 0/4", reasons[0])

    def test_gate_allowed_filter(self):
        rows = sweep({"off": {"W1": 50, "W2": 50}, "dflash7": {"W1": 90, "W2": 90},
                      "mtp2": {"W1": 60, "W2": 60}})
        s = sb.summarize(rows)
        winner, _ = sb.gate(s, sb.greedy_matches(rows, "sweep"), "sweep", allowed={"mtp2"})
        self.assertEqual(winner, "mtp2")

    def test_compare_before_after(self):
        rows = [row("before", "W1", 40, label="before"), row("after", "W1", 60, label="after"),
                row("before", "W3", 40, label="before")]
        pairs = sb.compare(sb.summarize(rows))
        self.assertEqual(pairs, {("W1", "greedy"): (40, 60, 1.5)})

    def test_report_mentions_winner_and_spend(self):
        rows = sweep({"off": {"W1": 50, "W2": 50}, "mtp2": {"W1": 65, "W2": 60}})
        rows.append({"kind": "spend", "label": "sweep", "usd": 1.25})
        text = sb.report_md(rows)
        self.assertIn("Gate winner: `mtp2`", text)
        self.assertIn("$1.25", text)

    def test_base_args_mirror_agent(self):
        self.assertEqual(sb.BASE_ARGS[-2:], ["--metrics", "--no-webui"])
        self.assertIn(sb.MODEL["file"], sb.BASE_ARGS[1])

    def test_workloads_are_pinned(self):
        ws = sb.workloads()
        self.assertEqual([w["id"] for w in ws], ["W1", "W2", "W3", "W4", "W5", "W6"])
        self.assertIn("fn last_line", ws[2]["messages"][0]["content"])


if __name__ == "__main__":
    unittest.main()
