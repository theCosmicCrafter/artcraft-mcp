#!/usr/bin/env python3
"""Static ArtCraft route-drift checker.

The checker has three independent extraction stages:

1. Parse the upstream artcraft-services Actix-web route registrations
   (nested ``web::scope`` / ``web::resource`` chains, direct scope
   ``.route()`` calls, and ``RouteBuilder`` additions).
2. Parse the MCP Rust tools for raw ``/v1`` URL construction, upstream
   ``artcraft_api_defs`` path constants, and ``artcraft_client`` endpoint
   helper usage.
3. Compare path templates and HTTP methods, then report missing routes,
   verb mismatches, healthy matches, and uncovered upstream surface area.

This is pure static analysis: it performs no network requests and spends no
ArtCraft credits.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
from collections import defaultdict
from dataclasses import dataclass, field
from pathlib import Path
from typing import Iterable, Optional

DEFAULT_UPSTREAM = r"C:\Users\richk\CascadeProjects\upstream\artcraft-services"
REPO_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_ROUTE_TABLE = REPO_ROOT / "tools" / "upstream_route_table.json"
MCP_TOOLS_DIR = REPO_ROOT / "rust" / "artcraft_mcp_cli" / "src" / "tools"

HTTP_METHODS = {"get", "post", "put", "delete", "patch", "head", "options", "trace", "connect"}
METHOD_SORT_ORDER = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS", "TRACE", "CONNECT"]
METHOD_RANK = {method: index for index, method in enumerate(METHOD_SORT_ORDER)}

RAW_HELPER_METHODS = {
    "raw_json_get": "GET",
    "raw_json_post": "POST",
    "raw_json_put": "PUT",
    "raw_json_delete": "DELETE",
    "raw_delete": "DELETE",
    "raw_multipart_post": "POST",
    "raw_multipart_post_named_file": "POST",
    "typed_json_post": "POST",
}

CLIENT_HELPER_METHODS = {
    "basic_json_get_request": "GET",
    "basic_json_post_request": "POST",
    "basic_json_delete_request": "DELETE",
    "basic_query_string_post_request": "POST",
    "http_get_anonymous": "GET",
}

PATH_CONSTANT_RE = re.compile(r"\b[A-Z][A-Z0-9_]*(?:_PATH|_URL_PATH)\b")
FUNCTION_RE = re.compile(r"(?m)^\s*(?:pub\s+)?(?:async\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(")
DISPATCH_RE = re.compile(
    # rustfmt wraps long match arms in a block:
    #   "tool" => {
    #       module::function(...).await
    #   }
    # so permit one optional opening brace before the callee.
    r"\"([A-Za-z0-9_]+)\"\s*=>\s*\{?\s*([A-Za-z_][A-Za-z0-9_]*)::([A-Za-z_][A-Za-z0-9_]*)\s*\(",
    re.MULTILINE,
)


# -----------------------------------------------------------------------------
# Small Rust lexer / Actix expression parser
# -----------------------------------------------------------------------------


def strip_comments(source: str) -> str:
    """Remove Rust // and /* */ comments while preserving byte positions.

    String contents are preserved. Rust block comments can nest, so this keeps a
    depth counter. The output always has the same length as the input, which
    lets every extracted token keep accurate source offsets and line numbers.
    """

    output = list(source)
    index = 0
    state = "normal"
    block_depth = 0

    while index < len(source):
        char = source[index]

        if state == "normal":
            if char == '"':
                state = "string"
                index += 1
            elif char == "/" and index + 1 < len(source) and source[index + 1] == "/":
                output[index] = output[index + 1] = " "
                index += 2
                state = "line_comment"
            elif char == "/" and index + 1 < len(source) and source[index + 1] == "*":
                output[index] = output[index + 1] = " "
                index += 2
                state = "block_comment"
                block_depth = 1
            else:
                index += 1

        elif state == "string":
            if char == "\\":
                index += 2
            elif char == '"':
                state = "normal"
                index += 1
            else:
                index += 1

        elif state == "line_comment":
            if char == "\n":
                state = "normal"
            else:
                output[index] = " "
            index += 1

        elif state == "block_comment":
            if char == "/" and index + 1 < len(source) and source[index + 1] == "*":
                output[index] = output[index + 1] = " "
                block_depth += 1
                index += 2
            elif char == "*" and index + 1 < len(source) and source[index + 1] == "/":
                output[index] = output[index + 1] = " "
                block_depth -= 1
                index += 2
                if block_depth == 0:
                    state = "normal"
            else:
                if char != "\n":
                    output[index] = " "
                index += 1

    return "".join(output)


@dataclass
class Token:
    kind: str
    value: str
    start: int
    end: int
    line: int


def lex_rust(source: str) -> list[Token]:
    """Lex just enough Rust for route-registration expression analysis."""

    tokens: list[Token] = []
    index = 0
    line = 1

    while index < len(source):
        char = source[index]

        if char == "\n":
            line += 1
            index += 1
            continue
        if char.isspace():
            index += 1
            continue

        start = index
        token_line = line

        if char == '"':
            index += 1
            value_chars: list[str] = []
            while index < len(source):
                current = source[index]
                if current == "\\" and index + 1 < len(source):
                    escape = source[index + 1]
                    escapes = {
                        "n": "\n",
                        "r": "\r",
                        "t": "\t",
                        "\\": "\\",
                        '"': '"',
                        "0": "\0",
                    }
                    value_chars.append(escapes.get(escape, escape))
                    index += 2
                elif current == '"':
                    index += 1
                    break
                else:
                    value_chars.append(current)
                    if current == "\n":
                        line += 1
                    index += 1
            tokens.append(Token("string", "".join(value_chars), start, index, token_line))
            continue

        if char.isalpha() or char == "_":
            index += 1
            while index < len(source) and (source[index].isalnum() or source[index] == "_"):
                index += 1
            tokens.append(Token("ident", source[start:index], start, index, token_line))
            continue

        if char.isdigit():
            index += 1
            while index < len(source) and (source[index].isalnum() or source[index] in "._"):
                index += 1
            tokens.append(Token("number", source[start:index], start, index, token_line))
            continue

        if source.startswith("::", index):
            tokens.append(Token("punct", "::", start, index + 2, token_line))
            index += 2
            continue

        tokens.append(Token("punct", char, start, index + 1, token_line))
        index += 1

    return tokens


@dataclass
class ActixRouteCall:
    method: str
    path_suffix: Optional[str]
    line: int
    handler: Optional[str]


@dataclass
class ActixNode:
    kind: str
    path: str
    start: int
    end: int
    line: int
    routes: list[ActixRouteCall] = field(default_factory=list)
    children: list["ActixNode"] = field(default_factory=list)


def is_web_expr(tokens: list[Token], index: int) -> bool:
    return (
        index + 2 < len(tokens)
        and tokens[index].value == "web"
        and tokens[index + 1].value == "::"
        and tokens[index + 2].value in {"scope", "resource"}
    )


def matching_paren(tokens: list[Token], open_index: int) -> Optional[int]:
    depth = 0
    for index in range(open_index, len(tokens)):
        value = tokens[index].value
        if value == "(":
            depth += 1
        elif value == ")":
            depth -= 1
            if depth == 0:
                return index
    return None


def parse_route_args(tokens: list[Token], start: int, end: int, line: int) -> Optional[ActixRouteCall]:
    """Parse the argument list of one Actix `.route(...)` call."""

    path_suffix: Optional[str] = None
    if start < end and tokens[start].kind == "string":
        # Scope-style registration: .route("/suffix", web::post().to(handler))
        path_suffix = tokens[start].value

    method: Optional[str] = None
    for index in range(start, max(start, end - 2)):
        if (
            tokens[index].value == "web"
            and tokens[index + 1].value == "::"
            and tokens[index + 2].value in HTTP_METHODS
        ):
            method = tokens[index + 2].value.upper()
            break

    if method is None:
        return None

    handler: Optional[str] = None
    for index in range(start, max(start, end - 2)):
        if tokens[index].value == "." and tokens[index + 1].value == "to" and tokens[index + 2].value == "(":
            close_index = matching_paren(tokens, index + 2)
            if close_index is not None:
                identifiers = [
                    tokens[inner].value
                    for inner in range(index + 3, min(close_index, end))
                    if tokens[inner].kind == "ident"
                ]
                if identifiers and identifiers[0] not in {"HttpResponse", "web"}:
                    handler = identifiers[0]
            break

    return ActixRouteCall(method=method, path_suffix=path_suffix, line=line, handler=handler)


def parse_web_expr(tokens: list[Token], index: int) -> Optional[ActixNode]:
    """Parse one `web::scope(...)` or `web::resource(...)` chain.

    This is intentionally not a complete Rust parser. It understands the
    chained-builder shape Actix route files use and treats method arguments as
    opaque except for `.service(...)` (recursive route expressions) and
    `.route(...)` (method registration).
    """

    if not is_web_expr(tokens, index):
        return None

    kind = tokens[index + 2].value
    cursor = index + 3
    if cursor >= len(tokens) or tokens[cursor].value != "(":
        return None

    path_close = matching_paren(tokens, cursor)
    if path_close is None or path_close <= cursor + 1 or tokens[cursor + 1].kind != "string":
        return None

    node = ActixNode(
        kind=kind,
        path=tokens[cursor + 1].value,
        start=tokens[index].start,
        end=tokens[path_close].end,
        line=tokens[index].line,
    )

    cursor = path_close + 1
    while cursor + 2 < len(tokens) and tokens[cursor].value == "." and tokens[cursor + 1].kind == "ident" and tokens[cursor + 2].value == "(":
        chain_method = tokens[cursor + 1].value
        open_index = cursor + 2
        close_index = matching_paren(tokens, open_index)
        if close_index is None:
            break

        if chain_method == "service":
            child: Optional[ActixNode] = None
            for inner in range(open_index + 1, max(open_index + 1, close_index - 2)):
                if is_web_expr(tokens, inner):
                    child = parse_web_expr(tokens, inner)
                    break
            if child is not None:
                node.children.append(child)

        elif chain_method == "route":
            route_call = parse_route_args(tokens, open_index + 1, close_index, tokens[cursor].line)
            if route_call is not None:
                node.routes.append(route_call)

        cursor = close_index + 1

    if cursor > 0:
        node.end = tokens[cursor - 1].end
    return node


def join_paths(prefix: str, suffix: str) -> str:
    if not prefix:
        return suffix or "/"
    if not suffix:
        return prefix
    return prefix.rstrip("/") + (suffix if suffix.startswith("/") else "/" + suffix)


def flatten_actix_node(node: ActixNode, prefix: str, output: list[dict], relative_file: str) -> None:
    node_path = join_paths(prefix, node.path)

    if node.kind == "resource":
        for route in node.routes:
            output.append(
                {
                    "path": node_path,
                    "method": route.method,
                    "line": route.line,
                    "handler": route.handler,
                    "file": relative_file,
                    "registration": "actix-resource",
                }
            )
        child_prefix = prefix
    else:
        for route in node.routes:
            output.append(
                {
                    "path": join_paths(node_path, route.path_suffix or ""),
                    "method": route.method,
                    "line": route.line,
                    "handler": route.handler,
                    "file": relative_file,
                    "registration": "actix-scope-route",
                }
            )
        child_prefix = node_path

    for child in node.children:
        flatten_actix_node(child, child_prefix, output, relative_file)


def unescape_simple_rust_string(value: str) -> str:
    escapes = {"n": "\n", "r": "\r", "t": "\t", "\\": "\\", '"': '"', "0": "\0"}
    output: list[str] = []
    index = 0
    while index < len(value):
        if value[index] == "\\" and index + 1 < len(value):
            output.append(escapes.get(value[index + 1], value[index + 1]))
            index += 2
        else:
            output.append(value[index])
            index += 1
    return "".join(output)


def extract_attribute_macro_endpoints(upstream_root: Path) -> dict[str, dict]:
    """Index endpoint functions declared with Actix `#[get("/path")]` macros.

    Most upstream routes are explicit `web::resource` registrations. A small
    number of legacy handlers instead rely on actix-web's attribute macro,
    which generates a service with the same name as the handler function and
    is mounted with `.service(handler_name)`.
    """

    root = upstream_root / "crates" / "service" / "web" / "storyteller_web" / "src" / "http_server" / "endpoints"
    endpoints: dict[str, dict] = {}
    if not root.is_dir():
        return endpoints

    verbs = "|".join(sorted(HTTP_METHODS))
    macro_re = re.compile(
        r"#\[(?P<method>" + verbs + r")\s*\(\s*\"(?P<path>(?:\\.|[^\"\\])*)\"\s*\)\s*\]"
        r"(?:\s*#\[[^\]]+\]\s*)*"
        r"\s*(?:pub\s+)?(?:async\s+)?fn\s+(?P<handler>[A-Za-z_][A-Za-z0-9_]*)",
        re.MULTILINE,
    )
    for path in sorted(root.rglob("*.rs")):
        if "test" in path.name:
            continue
        source = strip_comments(path.read_text(encoding="utf-8", errors="replace"))
        relative = path.relative_to(upstream_root).as_posix()
        for match in macro_re.finditer(source):
            handler = match.group("handler")
            endpoints[handler] = {
                "path": unescape_simple_rust_string(match.group("path")),
                "method": match.group("method").upper(),
                "handler": handler,
                "file": relative,
                "line": source.count("\n", 0, match.start()) + 1,
            }
    return endpoints


def parse_actix_registration_file(
    path: Path,
    upstream_root: Path,
    attribute_endpoints: Optional[dict[str, dict]] = None,
) -> list[dict]:
    original_source = path.read_text(encoding="utf-8", errors="replace")
    source = strip_comments(original_source)
    tokens = lex_rust(source)

    nodes: list[ActixNode] = []
    for index in range(0, max(0, len(tokens) - 2)):
        if is_web_expr(tokens, index):
            node = parse_web_expr(tokens, index)
            if node is not None:
                nodes.append(node)

    # Nested nodes are also encountered by the flat scan. Keep only outermost
    # nodes as roots; the parent parse already contains nested children.
    roots: list[ActixNode] = []
    for node in nodes:
        contained = any(
            other is not node and other.start < node.start and node.end <= other.end
            for other in nodes
        )
        if not contained:
            roots.append(node)

    relative_file = path.relative_to(upstream_root).as_posix()
    routes: list[dict] = []
    for root in roots:
        flatten_actix_node(root, "", routes, relative_file)

    # RouteBuilder is a project-local fluent wrapper around the same Actix
    # resource registrations. Its methods take complete paths rather than
    # scope-relative suffixes.
    route_builder_re = re.compile(
        r"\.add_(get|post|delete)\s*\(\s*\"((?:\\.|[^\"\\])*)\"\s*,\s*([A-Za-z_][A-Za-z0-9_:]*)"
    )
    for match in route_builder_re.finditer(source):
        method = match.group(1).upper()
        route_path = unescape_simple_rust_string(match.group(2))
        line = source.count("\n", 0, match.start()) + 1
        handler = match.group(3).split("::")[-1]
        routes.append(
            {
                "path": route_path,
                "method": method,
                "line": line,
                "handler": handler,
                "file": relative_file,
                "registration": "route-builder",
            }
        )
        # RouteBuilder::add_get and add_post always add a CORS/health HEAD
        # route. add_delete does so only when its third boolean is true.
        tail = source[match.end() : match.end() + 80]
        add_head = method in {"GET", "POST"} or not re.match(r"\s*,\s*false", tail)
        if add_head:
            routes.append(
                {
                    "path": route_path,
                    "method": "HEAD",
                    "line": line,
                    "handler": None,
                    "file": relative_file,
                    "registration": "route-builder-head",
                }
            )

    # Attribute-macro handlers are mounted directly as services. There are
    # only a couple today (`/alpha` and `/enable`), but scanning the mount
    # sites keeps the inventory complete without hardcoding those paths.
    if attribute_endpoints:
        service_re = re.compile(r"\.service\s*\(\s*([A-Za-z_][A-Za-z0-9_]*)\s*\)")
        for match in service_re.finditer(source):
            handler = match.group(1)
            endpoint = attribute_endpoints.get(handler)
            if endpoint is None:
                continue
            routes.append(
                {
                    "path": endpoint["path"],
                    "method": endpoint["method"],
                    "line": source.count("\n", 0, match.start()) + 1,
                    "handler": handler,
                    "file": relative_file,
                    "registration": "actix-attribute-service",
                }
            )

    return routes


# -----------------------------------------------------------------------------
# Upstream inventory extraction
# -----------------------------------------------------------------------------


def upstream_registration_files(upstream_root: Path) -> list[Path]:
    storyteller = upstream_root / "crates" / "service" / "web" / "storyteller_web" / "src" / "http_server"
    route_root = storyteller / "routes"
    endpoint_root = storyteller / "endpoints"

    files: list[Path] = []
    if route_root.is_dir():
        for path in sorted(route_root.rglob("*.rs")):
            if path.name.endswith("_tests.rs") or path.name.endswith("_test.rs"):
                continue
            files.append(path)

    # Actix `.configure(...)` can hide a registration inside an endpoint module.
    # Currently this is the passwordless login-challenge surface. Discover those
    # configure functions rather than hardcoding only today's file.
    if endpoint_root.is_dir():
        configure_re = re.compile(r"pub\s+fn\s+configure\s*\(\s*cfg\s*:\s*&mut\s*web::ServiceConfig")
        for path in sorted(endpoint_root.rglob("*.rs")):
            if "test" in path.name:
                continue
            try:
                text = path.read_text(encoding="utf-8", errors="replace")
            except OSError:
                continue
            if configure_re.search(text):
                files.append(path)

    # `add_application_routes()` mounts this component crate directly.
    billing_routes = upstream_root / "crates" / "service" / "plugins" / "billing" / "src" / "default_routes.rs"
    if billing_routes.is_file():
        files.append(billing_routes)

    # Stable de-duplication while preserving discovery order.
    unique: list[Path] = []
    seen: set[Path] = set()
    for path in files:
        resolved = path.resolve()
        if resolved not in seen:
            seen.add(resolved)
            unique.append(path)
    return unique


def normalize_route_path(path: str) -> str:
    path = path.split("?", 1)[0].split("#", 1)[0].strip()
    path = re.sub(r"/+", "/", path)
    if len(path) > 1:
        path = path.rstrip("/")
    return path or "/"


def canonicalize_route_template(path: str) -> str:
    """Canonicalize path parameter names for structural comparison."""

    path = normalize_route_path(path)
    segments = []
    for segment in path.split("/"):
        segments.append(re.sub(r"\{[^{}]*\}", "{}", segment))
    return "/".join(segments)


def route_family(path: str) -> str:
    parts = [part for part in normalize_route_path(path).split("/") if part]
    if not parts:
        return "root"
    if parts[0] == "v1":
        return parts[1] if len(parts) > 1 else "v1-root"
    return parts[0]


def sort_methods(methods: Iterable[str]) -> list[str]:
    return sorted(set(methods), key=lambda method: (METHOD_RANK.get(method, 99), method))


def consolidate_upstream_routes(entries: list[dict]) -> list[dict]:
    grouped: dict[str, dict] = {}

    for entry in entries:
        raw_path = normalize_route_path(entry["path"])
        canonical = canonicalize_route_template(raw_path)
        bucket = grouped.setdefault(
            canonical,
            {
                "path": raw_path,
                "canonical_path": canonical,
                "is_v1": raw_path == "/v1" or raw_path.startswith("/v1/"),
                "family": route_family(raw_path),
                "method_details": defaultdict(list),
            },
        )
        detail = {
            "file": entry["file"],
            "line": entry["line"],
            "handler": entry.get("handler"),
            "registration": entry.get("registration"),
        }
        method_details = bucket["method_details"][entry["method"]]
        if detail not in method_details:
            method_details.append(detail)

    routes: list[dict] = []
    for canonical, bucket in grouped.items():
        method_details = {
            method: sorted(details, key=lambda item: (item["file"], item["line"]))
            for method, details in sorted(
                bucket["method_details"].items(), key=lambda item: (METHOD_RANK.get(item[0], 99), item[0])
            )
        }
        methods = sort_methods(method_details.keys())
        primary_methods = sort_methods(method for method in methods if method != "HEAD")
        if not primary_methods:
            primary_methods = methods[:]

        source_set: set[str] = set()
        for details in method_details.values():
            for detail in details:
                source_set.add(detail["file"])

        routes.append(
            {
                "path": bucket["path"],
                "canonical_path": canonical,
                "is_v1": bucket["is_v1"],
                "family": bucket["family"],
                "methods": methods,
                "primary_methods": primary_methods,
                "method_details": method_details,
                "sources": sorted(source_set),
            }
        )

    routes.sort(key=lambda route: route["canonical_path"])
    return routes


def git_revision(path: Path) -> Optional[str]:
    try:
        result = subprocess.run(
            ["git", "-C", str(path), "rev-parse", "HEAD"],
            check=False,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            text=True,
            timeout=5,
        )
    except (OSError, subprocess.TimeoutExpired):
        return None
    revision = result.stdout.strip()
    return revision if result.returncode == 0 and revision else None


def extract_upstream_inventory(upstream_root: Path) -> tuple[list[dict], list[str]]:
    warnings: list[str] = []
    if not upstream_root.is_dir():
        raise FileNotFoundError(f"upstream checkout not found: {upstream_root}")

    entries: list[dict] = []
    files = upstream_registration_files(upstream_root)
    attribute_endpoints = extract_attribute_macro_endpoints(upstream_root)
    for path in files:
        try:
            entries.extend(parse_actix_registration_file(path, upstream_root, attribute_endpoints))
        except Exception as exc:  # keep the report useful if one file changes shape
            warnings.append(f"failed to parse upstream registration file {path}: {exc}")

    routes = consolidate_upstream_routes(entries)
    if not routes:
        warnings.append("upstream extraction produced zero routes")
    return routes, warnings


def build_route_table(routes: list[dict], upstream_root: Path) -> dict:
    v1_count = sum(1 for route in routes if route["is_v1"])
    return {
        "schema_version": 1,
        "source_of_truth": "actix-web route registrations in artcraft-services",
        "upstream_revision": git_revision(upstream_root),
        "route_count": len(routes),
        "v1_route_count": v1_count,
        "non_v1_route_count": len(routes) - v1_count,
        "routes": routes,
    }


# -----------------------------------------------------------------------------
# Upstream API-definition constants and artcraft_client endpoint index
# -----------------------------------------------------------------------------


def extract_api_def_path_constants(upstream_root: Path) -> dict[str, dict]:
    """Index upstream `*_PATH` constants by both short and module-qualified name.

    Upstream unfortunately reuses a few constant identifiers in sibling modules
    (for example the Gemini text/edit endpoint modules). A name-only index is
    therefore not authoritative; fully qualified uses must win.
    """

    root = upstream_root / "crates" / "api_clients" / "artcraft" / "artcraft_api_defs" / "src"
    by_name: dict[str, list[dict]] = defaultdict(list)
    by_qualified: dict[str, dict] = {}
    if not root.is_dir():
        return {"by_name": {}, "by_qualified": {}}

    const_re = re.compile(r"pub\s+const\s+([A-Z][A-Z0-9_]*(?:_PATH|_URL_PATH))\s*:\s*&str\s*=\s*\"((?:\\.|[^\"\\])*)\"")
    for path in sorted(root.rglob("*.rs")):
        source = strip_comments(path.read_text(encoding="utf-8", errors="replace"))
        relative = path.relative_to(upstream_root).as_posix()
        module_path = path.relative_to(root).with_suffix("").as_posix().replace("/", "::")
        for match in const_re.finditer(source):
            value = unescape_simple_rust_string(match.group(2))
            if not value.startswith("/"):
                continue
            name = match.group(1)
            detail = {
                "path": value,
                "file": relative,
                "line": source.count("\n", 0, match.start()) + 1,
                "module_path": module_path,
                "qualified_name": f"{module_path}::{name}",
            }
            by_name[name].append(detail)
            by_qualified[detail["qualified_name"]] = detail

    return {"by_name": dict(by_name), "by_qualified": by_qualified}


def parse_api_def_constant_imports(source: str) -> dict[str, list[str]]:
    """Map short API-defs constant imports to their defining Rust modules."""

    imports: dict[str, set[str]] = defaultdict(set)
    use_re = re.compile(r"(?ms)^\s*use\s+artcraft_api_defs::([^;]+);")
    constant_name_re = re.compile(r"^[A-Z][A-Z0-9_]*(?:_PATH|_URL_PATH)$")

    for match in use_re.finditer(source):
        body = " ".join(match.group(1).split())
        if "::{" in body:
            module_path, brace_body = body.split("::{", 1)
            brace_body = brace_body.rsplit("}", 1)[0]
            for item in brace_body.split(","):
                name = item.strip()
                if constant_name_re.fullmatch(name):
                    imports[name].add(module_path)
        else:
            segments = body.split("::")
            if len(segments) >= 2:
                name = segments[-1]
                if constant_name_re.fullmatch(name):
                    imports[name].add("::".join(segments[:-1]))

    return {name: sorted(paths) for name, paths in imports.items()}


def immediate_api_module_before(source: str, position: int) -> Optional[str]:
    """Return the module in an immediate fully qualified `...::CONSTANT` use."""

    prefix = source[max(0, position - 900) : position]
    match = re.search(
        r"(?:^|[^A-Za-z0-9_])artcraft_api_defs::((?:[A-Za-z_][A-Za-z0-9_]*::)+)\s*$",
        prefix,
    )
    if not match:
        return None
    return match.group(1).rstrip(":")


def resolve_api_constant(
    source: str,
    position: int,
    constant_name: str,
    imported_modules: dict[str, list[str]],
    api_constants: dict[str, dict],
) -> tuple[Optional[dict], str]:
    """Resolve a path constant, preferring its exact Rust module qualification."""

    by_qualified = api_constants.get("by_qualified", {})
    by_name = api_constants.get("by_name", {})

    module_path = immediate_api_module_before(source, position)
    if module_path:
        detail = by_qualified.get(f"{module_path}::{constant_name}")
        if detail:
            return detail, "fully-qualified"
        return None, f"missing-qualified:{module_path}::{constant_name}"

    for imported_module in imported_modules.get(constant_name, []):
        detail = by_qualified.get(f"{imported_module}::{constant_name}")
        if detail:
            return detail, "import-qualified"

    candidates = by_name.get(constant_name, [])
    if len(candidates) == 1:
        return candidates[0], "unique-name"
    if len(candidates) > 1:
        return None, "ambiguous-short-name"
    return None, "unresolved"


def is_use_statement_line(line_text: str) -> bool:
    return line_text.strip().startswith("use ")


def string_literals_in_source(source: str) -> Iterable[tuple[str, int, int, int]]:
    for token in lex_rust(source):
        if token.kind == "string":
            yield token.value, token.start, token.end, token.line


def extract_client_endpoint_index(upstream_root: Path, api_constants: dict[str, dict]) -> dict[str, dict]:
    root = upstream_root / "crates" / "api_clients" / "artcraft" / "artcraft_client" / "src" / "endpoints"
    endpoints: dict[str, dict] = {}
    if not root.is_dir():
        return endpoints

    for path in sorted(root.rglob("*.rs")):
        if path.name == "mod.rs":
            continue
        source = strip_comments(path.read_text(encoding="utf-8", errors="replace"))
        relative_module = path.relative_to(root).with_suffix("").as_posix().replace("/", "::")
        module_path = "endpoints::" + relative_module

        methods: set[str] = set()
        for helper, method in CLIENT_HELPER_METHODS.items():
            if re.search(r"\b" + re.escape(helper) + r"\s*\(", source):
                methods.add(method)
        # A handful of handwritten media upload endpoints use reqwest's
        # client directly instead of the small shared request helpers.
        for direct_match in re.finditer(r"\bclient\s*\.\s*(get|post|put|delete|patch)\s*\(", source):
            methods.add(direct_match.group(1).upper())

        paths: dict[str, dict] = {}
        imported_constant_modules = parse_api_def_constant_imports(source)

        # A few endpoint constants are base paths with a runtime token appended:
        # `format!("{}/{}", SOME_PATH, token)`. Treat the composite as the real
        # route rather than reporting the base path and the tiny `/{}` suffix
        # as two independent endpoints.
        composite_constant_positions: set[int] = set()
        composite_format_spans: list[tuple[int, int]] = []
        composite_re = re.compile(
            r"format!\s*\(\s*\"\{\}/\{\}\"\s*,\s*([A-Za-z0-9_:]*[A-Z][A-Z0-9_]*(?:_PATH|_URL_PATH))\s*,"
        )
        for composite_match in composite_re.finditer(source):
            expression = composite_match.group(1)
            constant_name = expression.split("::")[-1]
            constant_position = composite_match.start(1) + len(expression) - len(constant_name)
            constant, _resolution = resolve_api_constant(
                source,
                constant_position,
                constant_name,
                imported_constant_modules,
                api_constants,
            )
            if constant is None:
                continue
            composite_constant_positions.add(constant_position)
            composite_format_spans.append(composite_match.span())
            composite_path = constant["path"].rstrip("/") + "/{}"
            paths[composite_path] = {
                "kind": "api-defs-composite-path",
                "constant": constant["qualified_name"],
                "file": constant["file"],
                "line": constant["line"],
            }

        # Most generated upstream client endpoints pass an API-defs `*_PATH`
        # constant into one of the basic request helpers. Resolve it in the
        # endpoint's module context because upstream reuses a few short names.
        for constant_match in PATH_CONSTANT_RE.finditer(source):
            if constant_match.start() in composite_constant_positions:
                continue
            line_start = source.rfind("\n", 0, constant_match.start()) + 1
            line_end = source.find("\n", constant_match.end())
            if line_end == -1:
                line_end = len(source)
            if is_use_statement_line(source[line_start:line_end]):
                continue
            constant, _resolution = resolve_api_constant(
                source,
                constant_match.start(),
                constant_match.group(0),
                imported_constant_modules,
                api_constants,
            )
            if constant is not None:
                paths[constant["path"]] = {
                    "kind": "api-defs-constant",
                    "constant": constant["qualified_name"],
                    "file": constant["file"],
                    "line": constant["line"],
                }

        # Some handwritten client endpoints construct paths locally with
        # format!() (notably media upload/download and namespace-parameterized
        # credits/subscriptions). Reuse the MCP URL-fragment parser so both
        # root-relative literals and host-placeholder format strings count.
        for value, start, _end, line in string_literals_in_source(source):
            route_fragment = extract_route_fragment(value)
            if route_fragment == "/{}" and any(span_start <= start < span_end for span_start, span_end in composite_format_spans):
                continue
            if route_fragment is not None:
                paths.setdefault(
                    route_fragment,
                    {
                        "kind": "local-literal",
                        "constant": None,
                        "file": path.relative_to(upstream_root).as_posix(),
                        "line": line,
                    },
                )

        endpoints[module_path] = {
            "module_path": module_path,
            "file": path.relative_to(upstream_root).as_posix(),
            "methods": sort_methods(methods),
            "paths": [
                {"path": route_path, **detail}
                for route_path, detail in sorted(paths.items(), key=lambda item: item[0])
            ],
        }

    return endpoints


# -----------------------------------------------------------------------------
# MCP endpoint usage extraction
# -----------------------------------------------------------------------------


def find_functions(source: str) -> list[tuple[str, int, int, int]]:
    matches = list(FUNCTION_RE.finditer(source))
    functions: list[tuple[str, int, int, int]] = []
    for index, match in enumerate(matches):
        end = matches[index + 1].start() if index + 1 < len(matches) else len(source)
        line = source.count("\n", 0, match.start()) + 1
        functions.append((match.group(1), match.start(), end, line))
    return functions


def enclosing_function(functions: list[tuple[str, int, int, int]], position: int) -> Optional[tuple[str, int, int, int]]:
    for name, start, end, line in functions:
        if start <= position < end:
            return name, start, end, line
    return None


def parse_tool_dispatch(tools_dir: Path) -> dict[tuple[str, str], str]:
    mod_rs = tools_dir / "mod.rs"
    if not mod_rs.is_file():
        return {}
    source = strip_comments(mod_rs.read_text(encoding="utf-8", errors="replace"))
    return {(match.group(2), match.group(3)): match.group(1) for match in DISPATCH_RE.finditer(source)}


def infer_route_param_helpers(source: str, functions: list[tuple[str, int, int, int]]) -> dict[str, str]:
    """Infer MCP-local helper functions that forward a route-string parameter.

    The common shape is a small shared wrapper such as
    `upload_media_file(..., route_path: &str, ...)` or
    `enqueue(..., route: &str, ...)` which calls `client.absolute_url(param)`
    and then one raw HTTP helper. The parameter name is local convention, so
    discover it from the function signature rather than hardcoding `route_path`.
    """

    helpers: dict[str, str] = {}
    for name, start, end, _line in functions:
        body = source[start:end]
        open_brace = body.find("{")
        if open_brace == -1:
            continue
        signature = body[:open_brace]
        route_params = re.findall(r"(?:^|[,\(])\s*([A-Za-z_][A-Za-z0-9_]*)\s*:\s*&str\b", signature)
        if not route_params:
            continue

        forwards_route_param = any(
            re.search(r"\babsolute_url\s*\(\s*&?\s*" + re.escape(param) + r"\b", body)
            for param in route_params
        )
        if not forwards_route_param:
            continue

        for helper, method in RAW_HELPER_METHODS.items():
            if re.search(r"\b" + re.escape(helper) + r"\s*\(", body):
                helpers[name] = method
                break
    return helpers


def extract_route_fragment(value: str) -> Optional[str]:
    """Extract a root-relative API path from a URL-ish Rust string literal."""

    index = value.find("/")
    if index < 0:
        return None

    leading = value[:index]
    # URL format strings normally start with the host placeholder: "{}/v1/...".
    # Reject prose that merely mentions a path later in the sentence.
    if leading and not re.fullmatch(r"(?:\{[^{}]*\})*", leading):
        return None

    fragment = value[index:]
    fragment = fragment.split("?", 1)[0].split("#", 1)[0].strip()
    if not fragment.startswith("/"):
        return None
    if not re.fullmatch(r"/[A-Za-z0-9_./{}+-]*", fragment):
        return None
    if fragment == "/" or not re.search(r"[A-Za-z0-9_{}]", fragment):
        return None
    return fragment


def statement_start(source: str, position: int) -> int:
    return max(source.rfind(";", 0, position), source.rfind("{", 0, position), source.rfind("}", 0, position)) + 1


def assignment_variable_before(source: str, position: int) -> Optional[str]:
    prefix = source[max(0, position - 500) : position]
    matches = list(re.finditer(r"let\s+(?:mut\s+)?([A-Za-z_][A-Za-z0-9_]*)\s*=\s*([^;]*)$", prefix, re.DOTALL))
    if not matches:
        return None
    return matches[-1].group(1)


def first_method_in_text(text: str, variable: Optional[str] = None) -> Optional[str]:
    matches: list[tuple[int, str]] = []
    for helper, method in RAW_HELPER_METHODS.items():
        if variable:
            pattern = r"\b" + re.escape(helper) + r"\s*\(\s*&?\s*" + re.escape(variable) + r"\b"
        else:
            pattern = r"\b" + re.escape(helper) + r"\s*\("
        for match in re.finditer(pattern, text):
            matches.append((match.start(), method))
    if not matches:
        return None
    matches.sort()
    return matches[0][1]


def infer_method_for_literal(
    source: str,
    literal_start: int,
    function_bounds: Optional[tuple[str, int, int, int]],
    route_helpers: dict[str, str],
) -> tuple[Optional[str], str]:
    start = statement_start(source, literal_start)
    prefix = source[start:literal_start]

    # A local helper such as upload_media_file(..., route_path, ...) has its
    # method inferred from the helper body rather than from the call site.
    for helper_name, method in route_helpers.items():
        if re.search(r"\b" + re.escape(helper_name) + r"\s*\([^;]*$", prefix, re.DOTALL):
            return method, "route-helper"

    variable = assignment_variable_before(source, literal_start)
    if variable and function_bounds is not None:
        _name, _fn_start, fn_end, _fn_line = function_bounds
        method = first_method_in_text(source[literal_start:fn_end], variable)
        if method:
            return method, "variable-use"

    if function_bounds is not None:
        _name, _fn_start, fn_end, _fn_line = function_bounds
        # Same-statement first, then a bounded lookahead. This intentionally
        # stays conservative; unknown methods are reported rather than guessed.
        same_statement_end = source.find(";", literal_start, min(fn_end, literal_start + 700))
        if same_statement_end != -1:
            method = first_method_in_text(source[start:same_statement_end])
            if method:
                return method, "same-statement"
        method = first_method_in_text(source[literal_start : min(fn_end, literal_start + 700)])
        if method:
            return method, "nearby-use"

    return None, "undetermined"


def infer_method_near_position(
    source: str,
    position: int,
    function_bounds: Optional[tuple[str, int, int, int]],
) -> tuple[Optional[str], str]:
    start = statement_start(source, position)
    end = source.find(";", position)
    if end == -1 and function_bounds is not None:
        end = function_bounds[2]
    if end == -1:
        end = min(len(source), position + 500)
    method = first_method_in_text(source[start:end])
    if method:
        return method, "same-statement"
    if function_bounds is not None:
        method = first_method_in_text(source[position : min(function_bounds[2], position + 500)])
        if method:
            return method, "nearby-use"
    return None, "undetermined"


def line_number(source: str, position: int) -> int:
    return source.count("\n", 0, position) + 1


def add_usage(
    usages: dict[tuple[str, str], dict],
    path: str,
    method: Optional[str],
    source_file: str,
    line: int,
    function_name: Optional[str],
    tool_name: Optional[str],
    kind: str,
    detail: str,
) -> None:
    normalized = normalize_route_path(path)
    canonical = canonicalize_route_template(normalized)
    method_key = method or "UNKNOWN"
    key = (canonical, method_key)
    usage = usages.setdefault(
        key,
        {
            "path": normalized,
            "canonical_path": canonical,
            "method": method,
            "evidence": [],
            "tool_names": set(),
            "functions": set(),
        },
    )
    evidence = {
        "file": source_file,
        "line": line,
        "function": function_name,
        "tool": tool_name,
        "kind": kind,
        "detail": detail,
    }
    if evidence not in usage["evidence"]:
        usage["evidence"].append(evidence)
    if tool_name:
        usage["tool_names"].add(tool_name)
    if function_name:
        usage["functions"].add(function_name)


def extract_mcp_client_import_usage(
    source: str,
    module_name: str,
    relative_file: str,
    functions: list[tuple[str, int, int, int]],
    tool_dispatch: dict[tuple[str, str], str],
    client_index: dict[str, dict],
    usages: dict[tuple[str, str], dict],
    warnings: list[str],
) -> None:
    import_re = re.compile(
        r"(?m)^\s*use\s+artcraft_client::(endpoints::[A-Za-z0-9_:]+?)(?:\s+as\s+([A-Za-z_][A-Za-z0-9_]*))?\s*;"
    )

    for match in import_re.finditer(source):
        import_path = match.group(1)
        alias = match.group(2)
        segments = import_path.split("::")

        module_path: Optional[str] = None
        module_len = 0
        for length in range(len(segments), 0, -1):
            candidate = "::".join(segments[:length])
            if candidate in client_index:
                module_path = candidate
                module_len = length
                break
        if module_path is None:
            warnings.append(f"{relative_file}: could not resolve artcraft_client import {import_path}")
            continue

        imported_terminal = segments[-1]
        calls: list[re.Match[str]] = []
        if module_len == len(segments):
            # The import is a module; calls look like module::function(...).
            local_module = imported_terminal
            calls = list(re.finditer(r"\b" + re.escape(local_module) + r"\s*::\s*[A-Za-z_][A-Za-z0-9_]*\s*\(", source))
        else:
            # The import is a function, optionally renamed with `as`.
            local_function = alias or imported_terminal
            calls = list(re.finditer(r"\b" + re.escape(local_function) + r"\s*\(", source))

        if not calls:
            continue

        endpoint = client_index[module_path]
        endpoint_paths = endpoint.get("paths", [])
        endpoint_methods = endpoint.get("methods", [])
        if not endpoint_paths:
            warnings.append(f"{relative_file}: no upstream path extracted for client endpoint {module_path}")
            continue
        if len(endpoint_paths) > 1:
            warnings.append(f"{relative_file}: multiple upstream paths extracted for client endpoint {module_path}; using all")
        method = endpoint_methods[0] if len(endpoint_methods) == 1 else None
        if len(endpoint_methods) != 1:
            warnings.append(f"{relative_file}: ambiguous upstream method for client endpoint {module_path}")

        for call in calls:
            function = enclosing_function(functions, call.start())
            function_name = function[0] if function else None
            tool_name = tool_dispatch.get((module_name, function_name or ""))
            for path_detail in endpoint_paths:
                add_usage(
                    usages,
                    path_detail["path"],
                    method,
                    relative_file,
                    line_number(source, call.start()),
                    function_name,
                    tool_name,
                    "artcraft-client-endpoint",
                    module_path,
                )


def extract_mcp_usage(
    tools_dir: Path,
    api_constants: dict[str, dict],
    client_index: dict[str, dict],
) -> tuple[list[dict], list[str]]:
    warnings: list[str] = []
    usages: dict[tuple[str, str], dict] = {}
    tool_dispatch = parse_tool_dispatch(tools_dir)

    if not tools_dir.is_dir():
        raise FileNotFoundError(f"MCP tools directory not found: {tools_dir}")

    for path in sorted(tools_dir.glob("*.rs")):
        if path.name == "mod.rs":
            continue
        original_source = path.read_text(encoding="utf-8", errors="replace")
        source = strip_comments(original_source)
        relative_file = path.relative_to(REPO_ROOT).as_posix()
        module_name = path.stem
        functions = find_functions(source)
        route_helpers = infer_route_param_helpers(source, functions)
        imported_constant_modules = parse_api_def_constant_imports(source)

        # 1. Direct string-literal route construction. The lexer positions are
        # stable because strip_comments preserves input length exactly.
        for value, start, _end, token_line in string_literals_in_source(source):
            route_fragment = extract_route_fragment(value)
            if route_fragment is None:
                continue

            stmt_start = statement_start(source, start)
            constructor_prefix = source[stmt_start:start]
            helper_involved = any(
                re.search(r"\b" + re.escape(helper_name) + r"\s*\([^;]*$", constructor_prefix, re.DOTALL)
                for helper_name in route_helpers
            )
            constructor_involved = bool(re.search(r"(?:format!|absolute_url)\s*\([^;]*$", constructor_prefix, re.DOTALL))
            raw_call_involved = any(
                re.search(r"\b" + re.escape(helper) + r"\s*\([^;]*$", constructor_prefix, re.DOTALL)
                for helper in RAW_HELPER_METHODS
            )
            if not constructor_involved and not helper_involved and not raw_call_involved:
                continue

            function = enclosing_function(functions, start)
            function_name = function[0] if function else None
            tool_name = tool_dispatch.get((module_name, function_name or ""))
            method, method_evidence = infer_method_for_literal(source, start, function, route_helpers)
            add_usage(
                usages,
                route_fragment,
                method,
                relative_file,
                token_line,
                function_name,
                tool_name,
                "rust-url-literal",
                method_evidence,
            )

        # 2. Fully-qualified or imported upstream API-defs path constants used
        # directly by typed/raw MCP calls.
        for constant_match in PATH_CONSTANT_RE.finditer(source):
            line_start = source.rfind("\n", 0, constant_match.start()) + 1
            line_end = source.find("\n", constant_match.end())
            if line_end == -1:
                line_end = len(source)
            line_text = source[line_start:line_end].strip()
            if line_text.startswith("use "):
                continue

            constant_name = constant_match.group(0)
            constant, resolution = resolve_api_constant(
                source,
                constant_match.start(),
                constant_name,
                imported_constant_modules,
                api_constants,
            )
            if constant is None:
                # Most constants in type positions are not paths at all; only
                # warn when the surrounding statement is clearly a request or
                # the path constant itself is genuinely unresolvable.
                nearby = source[max(0, constant_match.start() - 150) : constant_match.end() + 150]
                if any(helper in nearby for helper in RAW_HELPER_METHODS):
                    warnings.append(
                        f"{relative_file}: unresolved API path constant {constant_name} ({resolution})"
                    )
                continue

            function = enclosing_function(functions, constant_match.start())
            function_name = function[0] if function else None
            tool_name = tool_dispatch.get((module_name, function_name or ""))
            method, method_evidence = infer_method_near_position(source, constant_match.start(), function)
            add_usage(
                usages,
                constant["path"],
                method,
                relative_file,
                line_number(source, constant_match.start()),
                function_name,
                tool_name,
                "api-defs-path-constant",
                f"{constant['qualified_name']}; {resolution}; {method_evidence}",
            )

        # 3. Endpoint functions in the upstream artcraft_client crate.
        extract_mcp_client_import_usage(
            source,
            module_name,
            relative_file,
            functions,
            tool_dispatch,
            client_index,
            usages,
            warnings,
        )

    output: list[dict] = []
    for usage in usages.values():
        usage["evidence"].sort(key=lambda item: (item["file"], item["line"]))
        usage["tool_names"] = sorted(usage["tool_names"])
        usage["functions"] = sorted(usage["functions"])
        usage["evidence_count"] = len(usage["evidence"])
        output.append(usage)
    output.sort(key=lambda item: (item["canonical_path"], item["method"] or "UNKNOWN"))
    return output, warnings


# -----------------------------------------------------------------------------
# Diff and report generation
# -----------------------------------------------------------------------------


def split_path_segments(path: str) -> list[str]:
    return [segment for segment in normalize_route_path(path).split("/") if segment]


def structural_match(usage_path: str, upstream_path: str) -> bool:
    usage_segments = split_path_segments(usage_path)
    upstream_segments = split_path_segments(upstream_path)
    if len(usage_segments) != len(upstream_segments):
        return False
    for usage_segment, upstream_segment in zip(usage_segments, upstream_segments):
        upstream_segment = re.sub(r"\{[^{}]*\}", "{}", upstream_segment)
        usage_segment = re.sub(r"\{[^{}]*\}", "{}", usage_segment)
        if upstream_segment == "{}":
            continue
        # A usage-side `{}` is an unknown runtime value. It can match an
        # upstream wildcard, but it is not evidence for a static upstream
        # literal segment.
        if usage_segment == "{}":
            return False
        if usage_segment != upstream_segment:
            return False
    return True


def find_upstream_match(usage_path: str, upstream_by_canonical: dict[str, dict], upstream_routes: list[dict]) -> Optional[dict]:
    canonical = canonicalize_route_template(usage_path)
    exact = upstream_by_canonical.get(canonical)
    if exact is not None:
        return exact

    candidates = [
        route
        for route in upstream_routes
        if structural_match(usage_path, route["canonical_path"])
    ]
    if not candidates:
        return None

    def specificity(route: dict) -> tuple[int, int, str]:
        static_segments = sum(1 for segment in split_path_segments(route["canonical_path"]) if segment != "{}")
        return (static_segments, -len(split_path_segments(route["canonical_path"])), route["canonical_path"])

    candidates.sort(key=specificity, reverse=True)
    return candidates[0]


def diff_routes(mcp_usages: list[dict], upstream_routes: list[dict]) -> dict:
    upstream_by_canonical = {route["canonical_path"]: route for route in upstream_routes}

    matched: list[dict] = []
    missing: list[dict] = []
    method_mismatches: list[dict] = []
    unknown_method_matches: list[dict] = []
    legacy_matches: list[dict] = []
    covered_canonicals: set[str] = set()

    for usage in mcp_usages:
        route = find_upstream_match(usage["canonical_path"], upstream_by_canonical, upstream_routes)
        base = {
            "path": usage["path"],
            "canonical_path": usage["canonical_path"],
            "method": usage["method"],
            "tool_names": usage["tool_names"],
            "functions": usage["functions"],
            "evidence": usage["evidence"],
        }
        if route is None:
            missing.append(base)
            continue

        covered_canonicals.add(route["canonical_path"])
        upstream_info = {
            "path": route["path"],
            "canonical_path": route["canonical_path"],
            "primary_methods": route["primary_methods"],
            "methods": route["methods"],
            "sources": route["sources"],
            "is_v1": route["is_v1"],
        }
        with_upstream = {**base, "upstream": upstream_info}

        if not route["is_v1"]:
            legacy_matches.append(with_upstream)

        method = usage["method"]
        if method is None:
            unknown_method_matches.append(with_upstream)
        elif method in route["primary_methods"] or method in route["methods"]:
            matched.append(with_upstream)
        else:
            method_mismatches.append(with_upstream)

    uncovered = [
        {
            "path": route["path"],
            "canonical_path": route["canonical_path"],
            "family": route["family"],
            "primary_methods": route["primary_methods"],
            "sources": route["sources"],
        }
        for route in upstream_routes
        if route["is_v1"] and route["canonical_path"] not in covered_canonicals
    ]

    uncovered_by_family: dict[str, list[dict]] = defaultdict(list)
    for route in uncovered:
        uncovered_by_family[route["family"]].append(route)

    return {
        "matched": matched,
        "missing": missing,
        "method_mismatches": method_mismatches,
        "unknown_method_matches": unknown_method_matches,
        "legacy_matches": legacy_matches,
        "uncovered": uncovered,
        "uncovered_by_family": dict(sorted(uncovered_by_family.items())),
    }


def summarize(mcp_usages: list[dict], upstream_routes: list[dict], diff: dict) -> dict:
    v1_routes = [route for route in upstream_routes if route["is_v1"]]
    covered_v1_count = len(v1_routes) - len(diff["uncovered"])
    usage_paths = {usage["canonical_path"] for usage in mcp_usages}
    return {
        "upstream_total_routes": len(upstream_routes),
        "upstream_v1_routes": len(v1_routes),
        "upstream_non_v1_routes": len(upstream_routes) - len(v1_routes),
        "mcp_usage_endpoints": len(mcp_usages),
        "mcp_usage_distinct_paths": len(usage_paths),
        "matched_usage_endpoints": len(diff["matched"]),
        "unknown_method_matched_usage_endpoints": len(diff["unknown_method_matches"]),
        "legacy_matched_usage_endpoints": len(diff["legacy_matches"]),
        "missing_route_usage_endpoints": len(diff["missing"]),
        "method_mismatch_usage_endpoints": len(diff["method_mismatches"]),
        "drift_usage_endpoints": len(diff["missing"]) + len(diff["method_mismatches"]),
        "covered_upstream_v1_routes": covered_v1_count,
        "uncovered_upstream_v1_routes": len(diff["uncovered"]),
    }


def format_evidence(evidence: list[dict], limit: int = 2) -> str:
    parts = []
    for item in evidence[:limit]:
        location = f"{item['file']}:{item['line']}"
        if item.get("tool"):
            location += f" [{item['tool']}]"
        parts.append(location)
    if len(evidence) > limit:
        parts.append(f"+{len(evidence) - limit} more")
    return ", ".join(parts)


def render_human_report(summary: dict, diff: dict, upstream_table: dict, verbose: bool = False) -> str:
    lines: list[str] = []
    revision = upstream_table.get("upstream_revision") or "unknown revision"
    lines.append("ArtCraft MCP route drift report")
    lines.append("=" * 34)
    lines.append(f"Upstream: {summary['upstream_total_routes']} routes ({summary['upstream_v1_routes']} /v1, {summary['upstream_non_v1_routes']} non-/v1) @ {revision}")
    lines.append(f"MCP usage: {summary['mcp_usage_endpoints']} endpoint+verb usages across {summary['mcp_usage_distinct_paths']} distinct paths")
    lines.append(
        "Status: "
        f"{summary['matched_usage_endpoints']} matched, "
        f"{summary['missing_route_usage_endpoints']} missing, "
        f"{summary['method_mismatch_usage_endpoints']} verb mismatches, "
        f"{summary['uncovered_upstream_v1_routes']} uncovered upstream /v1 routes"
    )
    if summary["unknown_method_matched_usage_endpoints"]:
        lines.append(f"Note: {summary['unknown_method_matched_usage_endpoints']} matched usages have statically undetermined HTTP verbs")
    if summary["legacy_matched_usage_endpoints"]:
        lines.append(f"Note: {summary['legacy_matched_usage_endpoints']} usages target legacy non-/v1 routes that still exist upstream")

    def render_usage_section(title: str, rows: list[dict], default_limit: int) -> None:
        lines.append("")
        lines.append(f"{title} ({len(rows)})")
        lines.append("-" * (len(title) + 3 + len(str(len(rows)))))
        if not rows:
            lines.append("  none")
            return
        limit = len(rows) if verbose else default_limit
        for row in rows[:limit]:
            method = row["method"] or "UNKNOWN"
            tools = ",".join(row["tool_names"]) if row["tool_names"] else "tool-unmapped"
            lines.append(f"  {method:7} {row['path']}  ({tools})")
            lines.append(f"          at {format_evidence(row['evidence'])}")
            if row.get("upstream"):
                upstream = row["upstream"]
                lines.append(f"          upstream {upstream['path']} methods {','.join(upstream['primary_methods'])}")
        if len(rows) > limit:
            lines.append(f"  ... {len(rows) - limit} more (use --verbose or JSON output)")

    render_usage_section("DRIFT: MCP calls routes missing upstream", diff["missing"], 30)
    render_usage_section("DRIFT: MCP verb not registered upstream", diff["method_mismatches"], 20)

    lines.append("")
    lines.append(f"Uncovered upstream /v1 routes ({len(diff['uncovered'])})")
    lines.append("----------------------------------------")
    if not diff["uncovered"]:
        lines.append("  none")
    else:
        families = diff["uncovered_by_family"]
        for family, routes in sorted(families.items(), key=lambda item: (-len(item[1]), item[0])):
            lines.append(f"  {family}: {len(routes)}")
            sample_count = len(routes) if verbose else 5
            for route in routes[:sample_count]:
                methods = ",".join(route["primary_methods"])
                lines.append(f"    {methods:11} {route['path']}")
            if len(routes) > sample_count:
                lines.append(f"    ... {len(routes) - sample_count} more")

    if verbose:
        render_usage_section("Healthy matched routes", diff["matched"], len(diff["matched"]))
        render_usage_section("Matched routes with unknown MCP verb", diff["unknown_method_matches"], len(diff["unknown_method_matches"]))
        render_usage_section("Legacy non-/v1 matched routes", diff["legacy_matches"], len(diff["legacy_matches"]))

    lines.append("")
    if summary["drift_usage_endpoints"]:
        lines.append(f"RESULT: drift detected ({summary['drift_usage_endpoints']} endpoint+verb usages); exiting non-zero")
    else:
        lines.append("RESULT: no route drift detected")
    return "\n".join(lines)


def jsonify_report(
    summary: dict,
    diff: dict,
    mcp_usages: list[dict],
    upstream_table: dict,
    warnings: list[str],
) -> dict:
    return {
        "schema_version": 1,
        "summary": summary,
        "upstream": {
            "revision": upstream_table.get("upstream_revision"),
            "route_count": upstream_table["route_count"],
            "v1_route_count": upstream_table["v1_route_count"],
            "non_v1_route_count": upstream_table["non_v1_route_count"],
        },
        "mcp_usage": mcp_usages,
        "drift": {
            "missing_routes": diff["missing"],
            "method_mismatches": diff["method_mismatches"],
        },
        "matched": diff["matched"],
        "unknown_method_matches": diff["unknown_method_matches"],
        "legacy_matches": diff["legacy_matches"],
        "uncovered_upstream_v1": diff["uncovered"],
        "uncovered_upstream_v1_by_family": diff["uncovered_by_family"],
        "extraction_warnings": warnings,
    }


# -----------------------------------------------------------------------------
# CLI
# -----------------------------------------------------------------------------


def parse_args(argv: Optional[list[str]] = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Statically compare ArtCraft MCP endpoint usage with upstream artcraft-services routes."
    )
    parser.add_argument(
        "--upstream",
        default=DEFAULT_UPSTREAM,
        help="Path to a local artcraft-services checkout (default: %(default)s)",
    )
    parser.add_argument(
        "--json",
        dest="json_out",
        help="Write the machine-readable drift report to this path",
    )
    parser.add_argument(
        "--route-table",
        default=str(DEFAULT_ROUTE_TABLE),
        help="Where to write the regenerated upstream route table (default: %(default)s)",
    )
    parser.add_argument(
        "--no-write-route-table",
        action="store_true",
        help="Do not rewrite the upstream route table artifact",
    )
    parser.add_argument(
        "--verbose",
        action="store_true",
        help="Print complete matched/uncovered sections instead of samples",
    )
    return parser.parse_args(argv)


def write_json(path: Path, data: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(data, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def main(argv: Optional[list[str]] = None) -> int:
    args = parse_args(argv)
    upstream_root = Path(args.upstream).expanduser().resolve()
    route_table_path = Path(args.route_table).expanduser()

    try:
        upstream_routes, upstream_warnings = extract_upstream_inventory(upstream_root)
    except Exception as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2

    upstream_table = build_route_table(upstream_routes, upstream_root)
    if not args.no_write_route_table:
        try:
            write_json(route_table_path, upstream_table)
        except OSError as exc:
            print(f"error: could not write route table {route_table_path}: {exc}", file=sys.stderr)
            return 2

    api_constants = extract_api_def_path_constants(upstream_root)
    client_index = extract_client_endpoint_index(upstream_root, api_constants)

    warnings = list(upstream_warnings)
    try:
        mcp_usages, usage_warnings = extract_mcp_usage(MCP_TOOLS_DIR, api_constants, client_index)
    except Exception as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2
    warnings.extend(usage_warnings)

    diff = diff_routes(mcp_usages, upstream_routes)
    summary = summarize(mcp_usages, upstream_routes, diff)
    report = jsonify_report(summary, diff, mcp_usages, upstream_table, warnings)

    print(render_human_report(summary, diff, upstream_table, verbose=args.verbose))

    if warnings:
        print("\nExtraction warnings:")
        for warning in warnings:
            print(f"  - {warning}")

    if args.json_out:
        json_path = Path(args.json_out).expanduser()
        try:
            write_json(json_path, report)
        except OSError as exc:
            print(f"error: could not write JSON report {json_path}: {exc}", file=sys.stderr)
            return 2
        print(f"\nMachine-readable report written to {json_path}")

    return 1 if summary["drift_usage_endpoints"] else 0


if __name__ == "__main__":
    raise SystemExit(main())
