# ADR-0003: Layered versioned TOML configuration

- Status: accepted (2026-10-08)
- Context: Old spec Part I mandated config-as-code with a schema version. We keep the spirit,
  drop the ceremony.
- Decision: Configuration is TOML with an explicit `config_version` integer (currently 1).
  Discovery layers, highest precedence last:
    1. platform user config dir (`~/.config/machie/config.toml` on Linux; OS-equivalent elsewhere)
    2. `$MACHIE_CONFIG` (explicit override)
    3. `./machie.toml` (project-local)
  Unknown keys are rejected at parse time (`deny_unknown_fields`) so typos fail loud.
  An unsupported `config_version` refuses to run with a friendly message rather than guessing.
- Consequences: every new config key must keep `config_version` semantics honest; breaking
  config changes bump the version and add a migration note here.
