import json
from pathlib import Path

root = Path(__file__).resolve().parents[1]
names = ("baseline", "candidate", "baseline-large", "candidate-large")
reports = {name: json.loads((root / f"{name}.json").read_text()) for name in names}
for report in reports.values():
    assert report["source_unchanged"] and report["before"] == report["after"]
for old, new in (("baseline", "candidate"), ("baseline-large", "candidate-large")):
    assert reports[old]["before"] == reports[new]["before"]
for name in ("baseline", "candidate", "candidate-large"):
    assert reports[name]["snapshot"]["ok"]
    assert reports[name]["snapshot"]["has_nonpublic"] is False
    assert [row["status"] for row in reports[name]["requests"]] == [200, 200, 200]
assert reports["baseline-large"]["snapshot"]["ok"] is False
assert "visibility snapshot exceeds supported limits" in reports["baseline-large"]["snapshot"]["error"]["message"]
assert [row["status"] for row in reports["baseline-large"]["requests"]] == [503, 503, 503]
expected = [(row["route"], row["bytes"], row["body_sha256"])
            for row in reports["baseline"]["requests"]]
for name in ("candidate", "candidate-large"):
    assert [(row["route"], row["bytes"], row["body_sha256"])
            for row in reports[name]["requests"]] == expected
receipt = {"ok": True, "all_corpus_hashes_unchanged": True,
           "small_public_responses_unchanged": True, "grown_public_responses_match_small": True,
           "grown_old_statuses": [503, 503, 503], "grown_new_statuses": [200, 200, 200],
           "real_copy_markdown_bytes": reports["baseline"]["before"]["markdown_bytes"],
           "synthetic_growth_markdown_bytes": reports["baseline-large"]["before"]["markdown_bytes"]}
(root / "comparison.json").write_text(json.dumps(receipt, indent=2) + "\n")
print(json.dumps(receipt, indent=2))
