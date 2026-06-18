# content_type

Angles swept: issues

The Content-Type set on uploaded S3 objects. The v2 CLI infers MIME type from file extension via Python `mimetypes.guess_type()`, overridable by `--content-type`, disableable by `--no-guess-mime-type` (which causes no ContentType to be sent, so S3 applies its `binary/octet-stream` default). Per-file inference applies on recursive uploads via `ProvideUploadContentTypeSubscriber`.

## Coverage

9 scenarios — 9 spec (5 mock+prod, 4 prod-only)

| Scenario (command / mode) | Spec | Classification |
|---|---|---|
| cp / upload — text extension (.html) | text_extensions.toml | spec |
| cp / upload — binary extension (.png) | binary_extensions.toml | spec |
| cp / upload — explicit --content-type override | explicit_override.toml | spec |
| cp / upload — extensionless file (no inference) | no_extension.toml | spec, prod-only |
| cp / upload — --no-guess-mime-type disables inference | no_guess_mime_type.toml | spec, prod-only |
| cp / upload — .yaml (not in macOS mimetypes DB) | yaml_config.toml | spec, prod-only |
| cp / upload — .woff font (platform-variant MIME) | woff_font.toml | spec |
| cp / upload — .wasm binary (platform-variant MIME) | wasm_binary.toml | spec |
| cp / recursive — per-file inference on mixed types | recursive_mixed.toml | spec, prod-only |

## Notes

- `woff_font` and `wasm_binary` carry `[deviation]` blocks: the Rust reimplementation returns different MIME variants (`application/font-woff` vs Python's `font/woff`; `application/wasm` is the same on this platform but Python may return None on others).
- `yaml_config` carries a `[deviation]` block: Python returns None for .yaml (S3 applies `binary/octet-stream`), the reimplementation returns `text/x-yaml`.
- `recursive_mixed` is `prod_only` because the extensionless file (`noext`) relies on S3's server-side `binary/octet-stream` default, which the mock does not apply.

**Mock/harness fidelity gaps:**

- **HG-006 — mock PutObject does not apply S3's default ContentType.** When no ContentType is sent in the PutObject request, real S3 stores `binary/octet-stream` (returned by HeadObject). The mock stores empty/None. This forces `no_extension`, `no_guess_mime_type`, `yaml_config`, and `recursive_mixed` to `prod_only`. Fix: apply `binary/octet-stream` as the default in the mock's PutObject handler when no ContentType header is present.
