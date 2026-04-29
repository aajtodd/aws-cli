# Smoke Testing Against Real S3

Manual-verification recipes that can't easily be captured as unit tests.
These exercise code paths that need a real network endpoint or real AWS
credentials. Use them when landing a change that touches SDK
configuration, credential flow, or TLS.

All commands assume you're in `crates/` and have run `cargo build` at
least once.

## Public Datasets (no credentials required)

Some AWS-hosted public datasets expose read access without authentication.
They're useful for exercising `--no-sign-request`, endpoint resolution,
and any code path where we want to prove we don't accidentally require
credentials.

| Bucket | Region | Contents | Source |
|--------|--------|----------|--------|
| `s3://noaa-goes16` | `us-east-1` | NOAA GOES-16 satellite imagery (live) | [registry.opendata.aws/noaa-goes](https://registry.opendata.aws/noaa-goes/) |
| `s3://noaa-goes17` | `us-east-1` | NOAA GOES-17 satellite imagery | [registry.opendata.aws/noaa-goes](https://registry.opendata.aws/noaa-goes/) |
| `s3://nasa-nex` | `us-west-2` | NASA NEX climate datasets | [registry.opendata.aws/nasanex](https://registry.opendata.aws/nasanex/) |
| `s3://commoncrawl` | `us-east-1` | Common Crawl web archive | [registry.opendata.aws/commoncrawl](https://registry.opendata.aws/commoncrawl/) |

The [AWS Registry of Open Data](https://registry.opendata.aws/) is the
canonical index. Buckets listed there are generally stable long-term,
but verify the bucket still exists before relying on a new one in docs.

### Recipe: `--no-sign-request`

```sh
# Clear any credentials from the environment to prove they're not used.
AWS_PROFILE="" AWS_ACCESS_KEY_ID="" AWS_SECRET_ACCESS_KEY="" \
  ./target/debug/aws --no-sign-request --region us-east-1 \
  s3 ls s3://noaa-goes16/
```

Expected: a prefix listing of GOES-16 data products. Exit 0.

### Sanity check: unsigned request to private bucket

Prove the request is truly unsigned (not silently falling back to
ambient credentials) by pointing `--no-sign-request` at a private bucket
you own:

```sh
AWS_PROFILE="" AWS_ACCESS_KEY_ID="" AWS_SECRET_ACCESS_KEY="" \
  ./target/debug/aws --no-sign-request --region us-east-2 \
  s3 ls s3://YOUR-PRIVATE-BUCKET/
```

Expected: `AccessDenied`. If you see objects, the request was somehow
signed and `--no-sign-request` is not working.

## TLS / CA Bundle

### Recipe: `--ca-bundle` is currently rejected

```sh
./target/debug/aws --ca-bundle /etc/ssl/cert.pem s3 ls
# → exit 252, stderr: "--ca-bundle is not currently supported. ..."
```

`--ca-bundle` is rejected at arg-parse time while we wait for upstream
TM to expose a `TlsContext` hook on `S3ClientConfig`. Honoring the flag
only on non-TM commands (ls/mb/rb/...) while silently no-op'ing on
cp/sync would be a compat trap; rejecting uniformly is safer. See
`compat.md` §`--ca-bundle`.

When the upstream change lands and we re-enable the flag, the following
recipes will apply:

```sh
# macOS system bundle — smoke test that our custom HTTP client is plugged in
AWS_PROFILE=your-profile ./target/debug/aws \
  --ca-bundle /etc/ssl/cert.pem --region us-east-2 s3 ls

# Linux
AWS_PROFILE=your-profile ./target/debug/aws \
  --ca-bundle /etc/ssl/certs/ca-certificates.crt --region us-east-2 s3 ls

# Unrelated self-signed cert — expect TLS failure on BOTH ls and cp
openssl req -x509 -newkey rsa:2048 -keyout /tmp/selfsigned.key \
  -out /tmp/selfsigned.pem -days 1 -nodes -subj "/CN=localhost"

AWS_PROFILE=your-profile ./target/debug/aws \
  --ca-bundle /tmp/selfsigned.pem --region us-east-2 s3 ls
# Expected: TLS verification failure.

AWS_PROFILE=your-profile ./target/debug/aws \
  --ca-bundle /tmp/selfsigned.pem --region us-east-2 \
  s3 cp /tmp/some-file.txt s3://your-bucket/test.txt
# Expected: TLS verification failure on the TM path too. If this
# succeeds, the TM fast path is bypassing our TlsContext — the whole
# reason we rejected the flag in the first place.
```

## Timeouts

### Recipe: `--cli-read-timeout` and `--cli-connect-timeout`

```sh
# Normal values — should just work
AWS_PROFILE=your-profile ./target/debug/aws \
  --cli-read-timeout 30 --cli-connect-timeout 10 \
  --region us-east-2 s3 ls

# Python-style "0 means disabled"
AWS_PROFILE=your-profile ./target/debug/aws \
  --cli-read-timeout 0 --cli-connect-timeout 0 \
  --region us-east-2 s3 ls

# Absurdly low connect timeout to force a timeout error
AWS_PROFILE=your-profile ./target/debug/aws \
  --cli-connect-timeout 1 --endpoint-url http://10.255.255.1:80 \
  --region us-east-2 s3 ls
# Expected: some form of connect-timeout error (the address is
# non-routable; the client should give up after ~1 second).
```

## Debug Output

### Recipe: `--debug`

```sh
./target/debug/aws --debug --region us-east-2 s3 ls 2>&1 | head -20
```

Expected: ANSI-colored tracing output on stderr with DEBUG-level
records from `aws_runtime`, `aws_smithy_runtime`, etc. Without
`--debug`, no such output appears.

## Transfer Operations (cp)

### Recipe: Upload and download against a real bucket

```sh
# Use a bucket you own in your default region
BUCKET=your-test-bucket

# Upload
echo "hello world" > /tmp/test.txt
AWS_PROFILE=your-profile ./target/debug/aws \
  s3 cp /tmp/test.txt s3://$BUCKET/test.txt

# Download
AWS_PROFILE=your-profile ./target/debug/aws \
  s3 cp s3://$BUCKET/test.txt /tmp/test-roundtrip.txt
diff /tmp/test.txt /tmp/test-roundtrip.txt

# Clean up
AWS_PROFILE=your-profile ./target/debug/aws \
  s3 rm s3://$BUCKET/test.txt
```

## Adding New Recipes

When you land a change that needs smoke verification:

1. Add the recipe here with the expected outcome.
2. Reference the recipe from the relevant entry in `compat.md`,
   `bosun.md`, or the PR description.
3. Recipes that become automatable migrate to the compat framework
   (`s3-compat-tests`) or to Rust integration tests. This doc is for
   the things that genuinely need a real endpoint or real credentials.
