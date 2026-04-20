# ext-mailersend

MailerSend transactional email provider for [Yeti](https://github.com/YetiRocks/yeti).

Registers a factory under the name `mailersend` so that yeti's top-level
`email:` config can resolve to this provider at startup:

```yaml
# yeti-config.yaml
email:
  provider:
    name: mailersend
    apiToken: "${MAILERSEND_TOKEN}"
    from: "no-reply@yeti.run"
    fromName: "Yeti"
    # endpoint is optional; defaults to api.mailersend.com/v1/email
```

## Usage

Add this crate to your yeti deployment's workspace dependencies:

```toml
# Cargo.toml
[workspace.dependencies]
ext-mailersend = { git = "https://github.com/YetiRocks/ext-mailersend", branch = "main" }
```

Then include `ext_mailersend::service()` in your static-app registry.
In the official yeti binary this is already wired — the extension
registers its factory during `Service::register()` and only actually
constructs a provider instance if the operator has selected
`mailersend` in config.

The sender domain must be verified in your MailerSend account — that's
where DKIM / SPF / DMARC live.

## Why hand-rolled?

MailerSend publishes official SDKs for PHP, Node, Python, Ruby, and Go
but not Rust as of 2026-04. Per yeti's [AGENTS.md "prefer established
crates" rule](https://github.com/YetiRocks/yeti/blob/main/AGENTS.md),
criterion #2 ("works out of the box") fails, so this ~60-line `reqwest`
client is the correct choice. Swap to an official Rust SDK the first
time one ships.

## Development

Local iteration requires a source checkout of both this crate and yeti.
Override the `yeti-sdk` git dep in a local `.cargo/config.toml`:

```toml
# .cargo/config.toml
[patch."https://github.com/YetiRocks/yeti"]
yeti-sdk = { path = "/path/to/yeti/crates/yeti-sdk" }
```

To test changes from yeti's tree:

```toml
# yeti's Cargo.toml (workspace root)
[patch."https://github.com/YetiRocks/ext-mailersend"]
ext-mailersend = { path = "/path/to/ext-mailersend" }
```

## License

MIT. See `LICENSE`.
