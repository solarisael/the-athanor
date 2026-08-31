import { existsSync, readFileSync } from "node:fs";
import path from "node:path";

const ISOLATED_POSTGRES_GATE = "SOLARISAEL_I_UNDERSTAND_THIS_IS_AN_ISOLATED_POSTGRES_TEST";
const OLD_POSTGRES_GATE = "SOLARISAEL_OMP_POSTGRES_TEST";
// Live Athanor topology and credentials. Any of these pointing at the real
// House means a test run could reach production state, so the suite refuses to
// start. This is decided purely from the process environment, so it holds from
// any working directory the tests are launched in.
const LIVE_CONFIGURATION_KEYS = [
  "ATHANOR_SUBSTRATE_EXE",
  "ATHANOR_SUBSTRATE_ROOT",
  "ATHANOR_STATE_DIR",
  "ATHANOR_AUTO",
  "ATHANOR_GIGA_ENABLED",
  "SOLARISAEL_SUBSTRATE_TEST_DATABASE_URL",
  "DATABASE_URL",
  "PGHOST",
  "PGPORT",
  "PGDATABASE",
  "PGUSER",
  "PGPASSWORD",
] as const;

// The pre-cutover topology names. They are inert at runtime by design, and that
// is precisely why a stale export must be reported: silently ignoring it lets
// an operator believe they have pointed the suite somewhere they have not.
const RENAMED_CONFIGURATION_KEYS: ReadonlyArray<readonly [string, string]> = [
  ["SOLARISAEL_HOUSE_RUST", "ATHANOR_SUBSTRATE_EXE"],
  ["SOLARISAEL_HOUSE_RUST_AUTO", "ATHANOR_AUTO"],
  ["SOLARISAEL_HOUSE_AUTO", "ATHANOR_AUTO"],
  ["SOLARISAEL_SUBSTRATE", "ATHANOR_SUBSTRATE_ROOT"],
  ["SOLARISAEL_STATE_DIR", "ATHANOR_STATE_DIR"],
  ["SOLARISAEL_GIGA_ENABLED", "ATHANOR_GIGA_ENABLED"],
  ["SOLARISAEL_HOUSE_CORE", "(removed; the core root is structural)"],
] as const;

function configured(key: string): boolean {
  return typeof process.env[key] === "string" && process.env[key]!.trim().length > 0;
}

function observedValue(key: string): string {
  const value = process.env[key] ?? "";
  if (/PASSWORD|DATABASE_URL|PGHOST|PGPORT|PGDATABASE|PGUSER/i.test(key)) return "[REDACTED]";
  return value.replace(/:\/\/[^/\s:@]+:[^/\s@]+@/g, "://[REDACTED]@");
}

function fail(message: string): never {
  throw new Error(`${message} Fix: clear the listed variables and rerun bun test.`);
}

function dotenvValue(root: string, key: string): string | null {
  const file = path.join(root, ".env");
  if (!existsSync(file)) return null;
  for (const line of readFileSync(file, "utf8").split(/\r?\n/)) {
    const match = line.match(new RegExp(`^\\s*${key}\\s*=\\s*(.*)\\s*$`));
    if (match) return match[1].trim().replace(/^(["'])(.*)\1$/, "$2");
  }
  return null;
}

function databaseIdentity(value: string): string | null {
  try {
    const url = new URL(value);
    return `${url.hostname.toLowerCase()}:${url.port || "5432"}${url.pathname.replace(/\/+$/, "") || "/"}`;
  } catch {
    return null;
  }
}

function dotenvDatabaseIdentity(root: string): string | null {
  const url = dotenvValue(root, "DATABASE_URL");
  if (url) return databaseIdentity(url);
  const host = dotenvValue(root, "PGHOST");
  const database = dotenvValue(root, "PGDATABASE");
  if (!host || !database) return null;
  return `${host.toLowerCase()}:${dotenvValue(root, "PGPORT") || "5432"}/${database}`;
}

function assertIsolatedPostgresTarget(): void {
  const substrate = process.env.ATHANOR_SUBSTRATE_ROOT?.trim();
  const testDatabase = process.env.SOLARISAEL_SUBSTRATE_TEST_DATABASE_URL?.trim();
  const executable = process.env.ATHANOR_SUBSTRATE_EXE?.trim();
  if (!substrate || !testDatabase || !executable) {
    fail(
      `${ISOLATED_POSTGRES_GATE}=1 requires ATHANOR_SUBSTRATE_ROOT, ATHANOR_SUBSTRATE_EXE, `
      + "and SOLARISAEL_SUBSTRATE_TEST_DATABASE_URL",
    );
  }
  const productionDatabaseIdentity = dotenvDatabaseIdentity(substrate);
  if (!productionDatabaseIdentity) {
    fail("The isolated PostgreSQL target has no complete .env database identity for comparison");
  }
  if (databaseIdentity(testDatabase) === null) {
    fail(`SOLARISAEL_SUBSTRATE_TEST_DATABASE_URL=${observedValue("SOLARISAEL_SUBSTRATE_TEST_DATABASE_URL")} is not a valid PostgreSQL DSN`);
  }
  if (databaseIdentity(testDatabase) === productionDatabaseIdentity) {
    fail(
      `SOLARISAEL_SUBSTRATE_TEST_DATABASE_URL=${observedValue("SOLARISAEL_SUBSTRATE_TEST_DATABASE_URL")} `
      + "matches the database identity in the configured substrate .env",
    );
  }
}

function assertNoRenamedConfiguration(): void {
  const stale = RENAMED_CONFIGURATION_KEYS.filter(([old]) => configured(old));
  if (stale.length === 0) return;
  const observed = stale
    .map(([old, replacement]) => `${old}=${observedValue(old)} -> use ${replacement}`)
    .join(", ");
  fail(
    "Refusing to run OMP tests with pre-cutover topology variables set; they are "
    + `inert at runtime and would silently do nothing: ${observed}.`,
  );
}

function assertNoLiveConfiguration(): void {
  if (configured(OLD_POSTGRES_GATE)) {
    fail(
      `${OLD_POSTGRES_GATE}=${observedValue(OLD_POSTGRES_GATE)} is no longer accepted; `
      + `use ${ISOLATED_POSTGRES_GATE}=1 with an explicit isolated test DSN`,
    );
  }
  assertNoRenamedConfiguration();
  const isolatedPostgres = process.env[ISOLATED_POSTGRES_GATE] === "1";
  const violations = LIVE_CONFIGURATION_KEYS.filter((key) => {
    if (isolatedPostgres && ["ATHANOR_SUBSTRATE_EXE", "ATHANOR_SUBSTRATE_ROOT", "SOLARISAEL_SUBSTRATE_TEST_DATABASE_URL"].includes(key)) return false;
    if (key === "ATHANOR_GIGA_ENABLED") return process.env[key] === "1";
    return configured(key);
  });
  if (violations.length > 0) {
    const observed = violations.map((key) => `${key}=${observedValue(key)}`).join(", ");
    fail(`Refusing to run OMP tests with live Athanor topology configuration: ${observed}.`);
  }
  if (isolatedPostgres) assertIsolatedPostgresTarget();
}

assertNoLiveConfiguration();

process.env.ATHANOR_GIGA_ENABLED = "0";

// The process-wide Insula writer stays silent for the whole suite: no test may
// post an observation to whatever Host a developer happens to have installed.
// Tests that exercise the writer construct their own `InsulaWriter`.
process.env.ATHANOR_DISABLE_INSULA = "1";

if (process.env[ISOLATED_POSTGRES_GATE] !== "1") {
  // Bun itself is a valid inert child for tests that exercise enabled-GIGA
  // validation; unlike the production binary it cannot open the substrate.
  process.env.ATHANOR_SUBSTRATE_EXE = process.execPath;
}
