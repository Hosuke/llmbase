"""Reproduce the Python baseline inventory without importing or executing it.

Run from the repository root: python3 docs/rust-migration/contracts/collect_contracts.py
This collector writes only baseline.json and inventory.md beside this file.
It is an inventory, not a proof that a native implementation is compatible.
"""

from __future__ import annotations

import ast
import hashlib
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[3]
OUT = Path(__file__).resolve().parent
PLUGIN_MODULES = {"browser", "cbeta", "ctext", "wikisource", "pdf", "sync"}
EXTENSION_MODULES = {"hooks", "web", "agent_api", "mcp_server", "cli", "refs"}
TESTS_BY_MODULE = {
    "anchor": ["test_anchor"], "atomic": ["test_atomic"], "chunk_cache": ["test_chunk_cache"],
    "compile": ["test_compile", "test_languages"], "config": ["test_languages"],
    "entities": ["test_entities"], "export": ["test_export", "test_languages"],
    "ingest": ["test_v064_features"], "lint": ["test_lint"],
    "llm": ["test_llm", "test_llm_meta", "test_xllm_key"],
    "mcp_server": ["test_query_promote"], "normalize": ["test_normalize"],
    "operations": ["test_sections", "test_query_promote", "test_xllm_key", "test_v064_features"],
    "pipeline": ["test_pipeline"], "query": ["test_query_promote", "test_query_prefilter", "test_v064_features"],
    "resolve": ["test_resolve"], "search": ["test_search_raw"], "sections": ["test_sections"],
    "split": ["test_split"], "taxonomy": ["test_languages", "test_web_languages"],
    "web": ["test_web_articles", "test_web_languages", "test_sections", "test_v064_features", "test_xllm_key"],
    "worker": ["test_v064_features", "test_languages"], "xici": ["test_languages", "test_web_languages"],
}
PRIVATE_CONSUMED = {
    "compile": {"_find_uncompiled", "_list_existing_concepts", "_write_article", "_merge_into", "_split_sections", "_assemble_sections", "_parse_compile_response"},
    "entities": {"_parse_entity_response", "_dedup_entities", "_save_entities"},
    "wikisource": {"_ingest_page"},
}


def source_record(path: Path, node: ast.AST) -> dict:
    return {"file": str(path.relative_to(ROOT)), "line": node.lineno, "end_line": node.end_lineno}


def expr(node):
    try:
        return ast.literal_eval(node)
    except (ValueError, TypeError):
        return ast.unparse(node)


def link(record):
    return f"[{record['file']}:{record['line']}](../../../{record['file']}#L{record['line']})"


def cell(value):
    return str(value).replace("|", "\\|").replace("\n", " ")


def test_links(module):
    names = TESTS_BY_MODULE.get(module.removeprefix("llmwiki.").split(".")[0], [])
    return ", ".join(f"[{n}](../../../tests/{n}.py)" for n in names) or "未見專項測試；須新增"


def main():
    inventory = {"baseline_head": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
                 "status": "pending", "cli": [], "http": [], "operations": [], "python": [], "tests": [], "file_sha256": {}}
    for path in sorted((ROOT / "llmwiki").rglob("*.py")):
        text = path.read_text()
        tree = ast.parse(text)
        module = ".".join(path.relative_to(ROOT).with_suffix("").parts).removesuffix(".__init__")
        short = module.removeprefix("llmwiki.").split(".")[0]
        inventory["file_sha256"][str(path.relative_to(ROOT))] = hashlib.sha256(path.read_bytes()).hexdigest()
        classification = "Python 擴展邊界" if short in PLUGIN_MODULES | EXTENSION_MODULES else "Rust 原生"
        for node in tree.body:
            if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)):
                if node.name.startswith("_") and node.name not in PRIVATE_CONSUMED.get(short, set()):
                    continue
                if path.name == "cli.py":
                    continue  # command handlers are exhaustively represented in CLI inventory
                signature = (f"{node.name}({ast.unparse(node.args)})" if hasattr(node, "args") else node.name)
                inventory["python"].append({**source_record(path, node), "module": module, "symbol": node.name,
                                            "signature": signature, "doc": (ast.get_docstring(node) or "").split("\n")[0],
                                            "classification": classification, "status": "pending"})
                if isinstance(node, ast.ClassDef):
                    for method in node.body:
                        if isinstance(method, (ast.FunctionDef, ast.AsyncFunctionDef)) and (not method.name.startswith("_") or method.name == "__init__"):
                            inventory["python"].append({**source_record(path, method), "module": module,
                                                       "symbol": f"{node.name}.{method.name}",
                                                       "signature": f"{node.name}.{method.name}({ast.unparse(method.args)})",
                                                       "doc": (ast.get_docstring(method) or "").split("\n")[0],
                                                       "classification": classification, "status": "pending"})
            if isinstance(node, ast.Assign) and any(isinstance(t, ast.Name) and t.id == "__all__" for t in node.targets):
                for name in expr(node.value):
                    inventory["python"].append({**source_record(path, node), "module": module, "symbol": name,
                                                "signature": name, "doc": "Package-level public re-export; preserve import identity and runtime patches",
                                                "classification": classification, "status": "pending"})
        for node in ast.walk(tree):
            if not isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
                continue
            for decorator in node.decorator_list:
                if not isinstance(decorator, ast.Call) or not isinstance(decorator.func, ast.Attribute):
                    continue
                owner = ast.unparse(decorator.func.value)
                if decorator.func.attr == "route" and owner == "app":
                    kwargs = {kw.arg: expr(kw.value) for kw in decorator.keywords}
                    inventory["http"].append({**source_record(path, node), "factory": module,
                                              "route": expr(decorator.args[0]), "methods": kwargs.get("methods", ["GET"]),
                                              "handler": node.name,
                                              "auth_decorator": any(ast.unparse(d) == "require_auth" for d in node.decorator_list),
                                              "status": "pending"})
                if path.name == "cli.py" and decorator.func.attr in {"group", "command"}:
                    args = []
                    for item in node.decorator_list:
                        if isinstance(item, ast.Call) and isinstance(item.func, ast.Attribute) and ast.unparse(item.func.value) == "click" and item.func.attr in {"argument", "option"}:
                            args.append({"kind": item.func.attr, "declarations": [expr(a) for a in item.args],
                                         "settings": {kw.arg: expr(kw.value) for kw in item.keywords}})
                    name = expr(decorator.args[0]) if decorator.args else node.name.replace("_", "-")
                    inventory["cli"].append({**source_record(path, node), "name": name, "parent": owner,
                                             "kind": decorator.func.attr, "params": args, "handler": node.name,
                                             "status": "pending"})
        if path.name == "operations.py":
            for node in ast.walk(tree):
                if isinstance(node, ast.Call) and isinstance(node.func, ast.Name) and node.func.id == "Operation":
                    record = {kw.arg: expr(kw.value) for kw in node.keywords}
                    record.setdefault("writes", False)
                    inventory["operations"].append({**source_record(path, node), **record, "status": "pending"})
    for path in sorted((ROOT / "tests").glob("test_*.py")):
        for node in ast.walk(ast.parse(path.read_text())):
            if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)) and node.name.startswith("test_"):
                inventory["tests"].append({**source_record(path, node), "name": node.name})
    inventory["counts"] = {key: len(inventory[key]) for key in ("cli", "http", "operations", "python", "tests")}
    (OUT / "baseline.json").write_text(json.dumps(inventory, ensure_ascii=False, indent=2) + "\n")
    lines = ["# 源碼抽取之完整契約清冊", "", "由 `collect_contracts.py` 以 AST 抽取；未 import 舊 runtime，未觸文庫。每列遷移狀態皆 pending。分類描述目標責任，不證明實作。函式表保留簽名、型別、defaults；回傳與副作用以 parity.md 及源碼為準。", "", "測試欄為同模組既有回歸測試起點，並非逐符號覆蓋證明；須逐案檢查 assertion 並補 native 對照測試。全部 CLI 仍缺獨立相容快照，全部 MCP 仍缺 stdio 協議全鏈測試。", "", f"基線 HEAD：`{inventory['baseline_head']}`。", "", "## CLI", "", "| 命令 | 參數／預設（源碼） | 分類／理由 | 測試 | 狀態 | 來源 |", "|---|---|---|---|---|---|"]
    for item in inventory["cli"]:
        command = "llmbase" if item["parent"] == "click" else "llmbase " + ("" if item["parent"] == "cli" else item["parent"] + " ") + item["name"]
        params = "; ".join(f"{p['declarations']}: {p['settings']}" for p in item["params"]) or "無額外參數；Click --help"
        is_plugin = item["parent"] == "ingest" and item["name"] in {"url", "pdf", "browse", "wikisource-learn", "wikisource-work", "cbeta-learn", "cbeta-status", "cbeta-work", "ctext-book", "ctext-catalog"}
        kind = "Python 擴展邊界（connector 可插件；CLI/儲存原生）" if is_plugin else "Rust 原生（通用能力）"
        lines.append(f"| `{command}` ({item['kind']}) | {cell(params)} | {kind} | CLI 契約測試缺口 | pending | {link(item)} |")
    lines += ["", "## HTTP（包括第三個 legacy search factory）", "", "Flask factory 為 Python 擴展邊界，通用路由底層皆須 Rust 原生；connector 處理可用明示插件。False 僅表無 require_auth decorator，非斷言整條路由無權限邏輯。", "", "| Factory | 方法／路徑 | Endpoint | require_auth decorator | 分類／理由 | 測試起點 | 狀態 | 來源 |", "|---|---|---|---|---|---|---|---|"]
    for item in inventory["http"]:
        kind = "Python 擴展邊界（Flask 外殼／静態站）" if not item["route"].startswith("/api/") else "Rust 原生（通用 route；Flask 轉譯）"
        lines.append(f"| `{item['factory']}` | `{','.join(item['methods'])} {item['route']}` | `{item['handler']}` | {item['auth_decorator']} | {kind} | {test_links(item['factory'])} | pending | {link(item)} |")
    lines += ["", "## MCP／operations", "", "每項 `params` 之 JSON Schema（含 required、default、enum、writeOnly）完整見 baseline.json；此表非重寫 schema。19 項皆 Rust 原生核心能力，ingest 的網站解析可交明示插件。writes=False 不代表沒有 file_back／promote／lint fix 等副作用。", "", "| 名稱 | Schema | writes／category | 分類 | 測試起點 | 狀態 | 來源 |", "|---|---|---|---|---|---|---|"]
    for item in inventory["operations"]:
        lines.append(f"| `{item['name']}` | `{cell(json.dumps(item['params'], ensure_ascii=False))}` | {item['writes']} / {item.get('category', 'general')} | Rust 原生 | {test_links('operations')} | pending | {link(item)} |")
    lines += ["", "## Python 公開符號與已知私有消費者", "", "此節各 symbol 均須按原簽名逐項驗證。`Python 擴展邊界` 的 facade 仍须將 storage/search/query/compile/jobs 轉入 Rust；非允許保留舊核心。傳輸與 callbacks 留 Python；connector parser 可留插件。新 native 實作驗證與現有測試對照見 parity.md。", "", "| Symbol／signature | 契約摘要（原 docstring） | 分類／責任 | 測試起點 | 狀態 | 來源 |", "|---|---|---|---|---|---|"]
    for item in inventory["python"]:
        lines.append(f"| `{item['module']}.{cell(item['signature'])}` | {cell(item['doc'])} | {item['classification']} | {test_links(item['module'])} | pending | {link(item)} |")
    lines += ["", "## 現有測試索引", "", "僅索引已存在 test function，未聲稱已執行或完整覆蓋。具體 test class 的 nodeid 仍依 pytest collection；baseline.json 留 file/line 可查。", "", "| 測試 | 來源 |", "|---|---|"]
    for item in inventory["tests"]:
        lines.append(f"| `{item['name']}` | {link(item)} |")
    (OUT / "inventory.md").write_text("\n".join(lines) + "\n")
    print(json.dumps(inventory["counts"]))


if __name__ == "__main__":
    main()
