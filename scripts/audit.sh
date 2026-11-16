#!/usr/bin/env bash
# Batería ejecutable de invariantes arquitectónicos del Virtual Workspace:
#   engine/ (motor base) <- service/ (models -> {stores, clients} -> services [DI] -> http [DI] -> {bootstrap, routes, main})
#
# Uso:
#   scripts/audit.sh            # ejecuta todo (checks duros + inspección)
#   scripts/audit.sh --quiet    # solo reporta violaciones

set -uo pipefail

cd "$(dirname "$0")/.." || exit 1

QUIET=0
[[ "${1:-}" == "--quiet" ]] && QUIET=1

FAILURES=0
RS=('--include=*.rs')
ROOTS=(service/src engine/src)

shopt -s nullglob
CLIENT_DIRS=()
[[ -d service/src/clients ]] && CLIENT_DIRS=(service/src/clients)
STORE_DIRS=()
[[ -d service/src/stores ]] && STORE_DIRS=(service/src/stores)

grep() {
  local rc=0
  command grep "$@" || rc=$?
  if [[ $rc -gt 1 ]]; then
    printf 'AUDIT-GREP-ERROR (exit %s): grep %s\n' "$rc" "$*" | tee /dev/stderr
  fi
  return "$rc"
}

fail_block() {
  local id="$1" desc="$2" out="$3"
  if [[ -n "$out" ]]; then
    printf '\n[FAIL] %-3s %s\n' "$id" "$desc"
    printf '%s\n' "$out" | sed 's/^/        /'
    FAILURES=$((FAILURES + 1))
  elif [[ $QUIET -eq 0 ]]; then
    printf '[ ok ] %-3s %s\n' "$id" "$desc"
  fi
}

inspect_block() {
  local id="$1" desc="$2" out="$3"
  [[ $QUIET -eq 1 ]] && return 0
  printf '\n[insp] %-3s %s\n' "$id" "$desc"
  if [[ -n "$out" ]]; then
    printf '%s\n' "$out" | sed 's/^/        /'
  else
    printf '        (sin coincidencias)\n'
  fi
}

TEST_INDEX="$(mktemp "${TMPDIR:-/tmp}/audit_test_lines.XXXXXX")"
LOG_INDEX="$(mktemp "${TMPDIR:-/tmp}/audit_log_lines.XXXXXX")"
trap 'rm -f "$TEST_INDEX" "$LOG_INDEX"' EXIT

find "${ROOTS[@]}" -name '*.rs' -print0 | xargs -0 awk '
  FNR == 1 { intest = 0; indent = "" }
  !intest && /^[[:space:]]*#\[cfg\((test|all\(test|any\(test)/ {
    intest = 1
    indent = $0
    sub(/#\[.*$/, "", indent)
  }
  intest { print FILENAME ":" FNR }
  intest && $0 == indent "}" { intest = 0 }
' > "$TEST_INDEX"

exclude_tests() {
  awk -v idx="$TEST_INDEX" '
    BEGIN { while ((getline line < idx) > 0) skip[line] = 1 }
    { split($0, p, ":"); if (!((p[1] ":" p[2]) in skip)) print }
  '
}

find "${ROOTS[@]}" -name '*.rs' -print0 | xargs -0 awk -v cap=60 '
  FNR == 1 { inlog = 0; run = 0 }
  !inlog && /(tracing|log)::(trace|debug|info|warn|error|event)![(]/ { inlog = 1; run = 0 }
  inlog {
    print FILENAME ":" FNR
    run++
    if ($0 ~ /;[[:space:]]*$/ || $0 ~ /^[[:space:]]*}[[:space:]]*$/ || run > cap) inlog = 0
  }
' > "$LOG_INDEX"

exclude_logs() {
  awk -v idx="$LOG_INDEX" '
    BEGIN { while ((getline line < idx) > 0) skip[line] = 1 }
    { split($0, p, ":"); if (!((p[1] ":" p[2]) in skip)) print }
  '
}

instrument_without_skip_all() {
  find "${ROOTS[@]}" -name '*.rs' -print0 | xargs -0 awk '
    /#\[(tracing::)?instrument/ {
      buf = $0; start = FNR
      while (buf !~ /\)\]/ && buf !~ /instrument\]/ && (getline > 0)) { buf = buf " " $0 }
      gsub(/[ \t]+/, " ", buf)
      if (buf !~ /skip_all/) print FILENAME ":" start ": " buf
    }
  '
}

# Detecta ciclos entre unidades de `service/src/services/`:
# - Un dominio multi-archivo (`services/house/{mod,reads,tasks/curation}.rs`) agrupa sus submódulos bajo el nodo `house`.
# - Las subcarpetas organizativas `services/shared/<u...>` y `services/compound/<u...>` agrupan cada unidad como `shared::<u1>` / `compound::<u1>`.
detect_service_cycles() {
  local svc_files=()
  while IFS= read -r -d '' f; do
    svc_files+=("$f")
  done < <(find service/src/services -name '*.rs' -print0)
  [[ ${#svc_files[@]} -eq 0 ]] && return 0

  awk '
    function unit_id(path,    n, segs) {
      sub(/^service\/src\/services\//, "", path)
      sub(/\/mod\.rs$/, "", path)
      sub(/\.rs$/, "", path)
      n = split(path, segs, /\//)
      if ((segs[1] == "shared" || segs[1] == "compound") && n >= 2) {
        return segs[1] "::" segs[2]
      }
      return segs[1]
    }
    function unit_from_import(tok,    n, segs) {
      n = split(tok, segs, /::/)
      if ((segs[1] == "shared" || segs[1] == "compound") && n >= 2) {
        return segs[1] "::" segs[2]
      }
      return segs[1]
    }
    FNR == 1 {
      cur = unit_id(FILENAME)
      is_root_mod = (FILENAME == "service/src/services/mod.rs" || FILENAME == "service/src/services/shared/mod.rs" || FILENAME == "service/src/services/compound/mod.rs")
      if (!is_root_mod) mod_known[cur] = 1
      in_use = 0; buf = ""
    }
    !is_root_mod && /^[[:space:]]*pub[[:space:]]+(struct|enum|trait|type)[[:space:]]+[A-Z][A-Za-z0-9_]*/ {
      line = $0
      sub(/^[[:space:]]*pub[[:space:]]+(struct|enum|trait|type)[[:space:]]+/, "", line)
      sub(/[^A-Za-z0-9_].*$/, "", line)
      sym_owner[line] = cur
    }
    !is_root_mod && /^[[:space:]]*(pub[[:space:]]+)?use[[:space:]]+crate::services::/ {
      in_use = 1; buf = $0
      if (buf ~ /;/) { record_use(cur, buf); in_use = 0; buf = "" }
      next
    }
    !is_root_mod && in_use {
      buf = buf " " $0
      if (buf ~ /;/) { record_use(cur, buf); in_use = 0; buf = "" }
    }
    function record_use(from, stmt) {
      use_count++
      use_from[use_count] = from
      use_stmt[use_count] = stmt
    }
    END {
      for (idx = 1; idx <= use_count; idx++) {
        from = use_from[idx]
        stmt = use_stmt[idx]
        sub(/^.*crate::services::/, "", stmt)
        gsub(/[{};,]/, " ", stmt)
        n = split(stmt, parts, /[[:space:]]+/)
        for (i = 1; i <= n; i++) {
          tok = parts[i]
          if (tok == "" || tok == "self" || tok == "super" || tok == "as") continue
          target = ""
          cand = unit_from_import(tok)
          if (cand in mod_known) {
            target = cand
          } else {
            last = tok
            sub(/^.*::/, "", last)
            if (last in sym_owner) target = sym_owner[last]
          }
          if (target != "" && target != from) {
            edge[from, target] = 1
            nodes[from] = 1
            nodes[target] = 1
          }
        }
      }
      for (u in nodes) state[u] = 0
      for (u in nodes) {
        if (state[u] == 0) dfs(u, u)
      }
    }
    function dfs(u, path,    k, sep, v) {
      state[u] = 1
      for (k in edge) {
        split(k, sep, SUBSEP)
        if (sep[1] == u) {
          v = sep[2]
          if (state[v] == 1) {
            print "ciclo detectado: " path " -> " v
          } else if (state[v] == 0) {
            dfs(v, path " -> " v)
          }
        }
      }
      state[u] = 2
    }
  ' "${svc_files[@]}"
}

# Verifica que ninguna declaración pública en `service/src/stores/` acepte ni exponga tipos crudos de Mongo/BSON.
store_public_leaks() {
  [[ ${#STORE_DIRS[@]} -eq 0 ]] && return 0
  find "${STORE_DIRS[@]}" -name '*.rs' -print0 | xargs -0 awk '
    FNR == 1 { bson_doc = 0 }
    /use[[:space:]]+.*(engine::db|bson).*Document\b/ && $0 !~ /Document[[:space:]]+as[[:space:]]+/ { bson_doc = 1 }
    /^[[:space:]]*pub([[:space:]]|\()/ && $0 !~ /^[[:space:]]*pub[[:space:]]+(mod|use)\b/ {
      buf = $0; start = FNR
      while (buf !~ /[{;]/ && (getline > 0)) {
        buf = buf " " $0
      }
      sig = buf
      sub(/\{.*$/, "", sig)
      gsub(/[ \t]+/, " ", sig)
      if (sig ~ /\b(Bson|Collection|Database|MongoDb)\b/ || sig ~ /bson::Document/ || (bson_doc && sig ~ /\bDocument\b/)) {
        print FILENAME ":" start ": " sig
      }
    }
  '
}

# ---------------------------------------------------------------------------
# Checks duros
# ---------------------------------------------------------------------------

fail_block 1 "unwrap()/expect() en código de aplicación (tests exentos)" \
  "$(grep -rnE '\bunwrap\s*\(|\bexpect\s*\(' "${ROOTS[@]}" "${RS[@]}" | grep -v 'unwrap_or' | exclude_tests)"

fail_block 2 "construcción cruda de cliente reqwest fuera de engine::stores::net" \
  "$(grep -rnE 'reqwest::Client(::new|::builder|Builder)|\bClient::(new|builder)\(' "${ROOTS[@]}" "${RS[@]}" | grep -vE '^engine/src/stores/net\.rs:')"

fail_block 3 "sintaxis de ruta axum con dos puntos (usar {id})" \
  "$(grep -rnE 'route\("[^"]*:' "${ROOTS[@]}" "${RS[@]}")"

fail_block 4 "TimeoutLayer de tower-http (rompe el envelope uniforme)" \
  "$(grep -rn 'TimeoutLayer' "${ROOTS[@]}" "${RS[@]}")"

fail_block 5 "models importando I/O o capas superiores (stores, clients, services, http, bootstrap, routes, engine::db, engine::stores, engine::web)" \
  "$(grep -rnE 'crate::(stores|clients|services|http|bootstrap|routes)\b|engine::(db|stores|web)\b' service/src/models "${RS[@]}" | grep -vE '^[^:]+:[0-9]+:\s*(//|//!)')"

store_edge_violations() {
  if [[ ${#STORE_DIRS[@]} -eq 0 ]]; then
    return 0
  fi
  grep -rnE 'crate::(clients|services|http|bootstrap|routes)\b|engine::web\b' "${STORE_DIRS[@]}" "${RS[@]}"
  while IFS= read -r -d '' f; do
    [[ "$f" == "service/src/stores/mod.rs" ]] && continue
    sname="${f#service/src/stores/}"
    sname="${sname%%/*}"
    sname="${sname%.rs}"
    grep -HnE 'crate::stores::[a-z0-9_]+' "$f" | grep -v "crate::stores::$sname\b"
  done < <(find "${STORE_DIRS[@]}" -name '*.rs' -print0)
}
fail_block 6 "stores importando services/http/clients/web o a otro módulo de stores hermano" \
  "$(store_edge_violations | grep -vE '^[^:]+:[0-9]+:\s*(//|//!|/\*|\*)')"

client_edge_violations() {
  if [[ ${#CLIENT_DIRS[@]} -gt 0 ]]; then
    grep -rnE 'crate::(stores|services|http|bootstrap|routes)\b|engine::(db|stores|web)\b' "${CLIENT_DIRS[@]}" "${RS[@]}"
    while IFS= read -r -d '' f; do
      [[ "$f" == "service/src/clients/mod.rs" ]] && continue
      cname="${f#service/src/clients/}"
      cname="${cname%%/*}"
      cname="${cname%.rs}"
      grep -HnE 'crate::clients::[a-z0-9_]+' "$f" | grep -v "crate::clients::$cname\b"
    done < <(find "${CLIENT_DIRS[@]}" -name '*.rs' -print0)
  fi
}
fail_block 7 "clients importando stores/services/http/db/web o a otro client hermano" \
  "$(client_edge_violations | grep -vE '^[^:]+:[0-9]+:\s*(//|//!|/\*|\*)')"

fail_block 8 "services importando http/bootstrap/routes/web o DTOs HTTP" \
  "$(grep -rnE 'crate::(http|bootstrap|routes)\b|engine::web\b|\b[A-Za-z0-9_]+Dto\b' service/src/services "${RS[@]}" | grep -vE '^[^:]+:[0-9]+:\s*(//|//!)')"

fail_block 9 "services instanciando *Store/*Repository/*Client/*Service directamente sin DI (ensamblar en bootstrap.rs)" \
  "$(grep -rnE '\b[A-Z][A-Za-z0-9_]*(Store|Repository|Repo|Client|Service)::new\s*\(' service/src/services "${RS[@]}" \
     | grep -vE '^[^:]+:[0-9]+:\s*(//|//!|/\*|\*)' \
     | exclude_tests)"

fail_block 10 "importaciones cíclicas entre servicios (DFS multinivel sobre service/src/services)" \
  "$(detect_service_cycles)"

fail_block 11 "http accediendo a stores/clients/db o instanciando servicios sin DI (usar State<*Service>)" \
  "$(grep -rnE 'crate::(stores|clients)\b|engine::(db|stores)\b|\bMongo::|\b[A-Z][A-Za-z0-9_]*(Store|Repository|Repo|Client|Service)::new\s*\(' service/src/http "${RS[@]}" \
     | grep -vE '^[^:]+:[0-9]+:\s*(//|//!)')"

fail_block 12 "primitivas de persistencia o BSON (Mongo:: / MongoDb / Collection / Database / Bson / doc! / engine::db) fuera de main.rs y service/src/stores/" \
  "$(grep -rnE '\b(Mongo|MongoDb|Collection|Database|Bson|IndexModel)\b|bson::|mongodb::|engine::(db|stores::(db|mongo))\b|\bdoc!\s*\(|\.(mongo|raw)\(\)' \
     service/src/models service/src/services service/src/http "${CLIENT_DIRS[@]}" "${RS[@]}" \
     | grep -vE '^[^:]+:[0-9]+:\s*(//|//!|/\*|\*)' \
     | exclude_tests)"

fail_block 13 "extractores crudos Query<T>/Path<T> fuera de engine/src/web/extractors/params.rs" \
  "$(grep -rnE '\b(Query|Path)<' "${ROOTS[@]}" "${RS[@]}" | grep -vE '^engine/src/web/extractors/params\.rs:')"

fail_block 14 "rutas totalmente calificadas en línea en service/src/ (los use van arriba)" \
  "$(grep -rnE '(crate::models|crate::stores|crate::services|crate::http|crate::clients)::' service/src "${RS[@]}" \
     | grep -vE '^[^:]+:[0-9]+:\s*(pub(\([a-z: ]+\))? )?use ' \
     | grep -vE '^[^:]+:[0-9]+:\s*(//|//!|/\*)')"

fail_block 15 "#[allow(clippy::too_many_arguments)] o #[allow(clippy::should_implement_trait)]" \
  "$(grep -rnE 'clippy::(too_many_arguments|should_implement_trait)' "${ROOTS[@]}" "${RS[@]}")"

fail_block 16 "#[tracing::instrument] sin skip_all" \
  "$(instrument_without_skip_all)"

fail_block 17 "tokio::spawn en código de dominio/http (Cloud Run congela CPU fuera del request)" \
  "$(grep -rnE '\btokio::(task::)?spawn\b|\bspawn\(' service/src/models "${STORE_DIRS[@]}" service/src/services service/src/http "${CLIENT_DIRS[@]}" "${RS[@]}" \
     | grep -vE '^[^:]+:[0-9]+:\s*(//|//!|/\*|\*)')"

fail_block 18 "punto flotante crudo (f32/f64) en stores/services/http (encapsular magnitudes físicas en models/ y usar Money entero)" \
  "$(grep -rnE '\b(f32|f64)\b' "${STORE_DIRS[@]}" service/src/services service/src/http "${RS[@]}" \
     | grep -vE '^[^:]+:[0-9]+:\s*(//|//!|/\*|\*)')"

fail_block 19 "construcción de payloads con json! fuera de logging/telemetry" \
  "$(grep -rnE '\bjson!\s*\(' "${ROOTS[@]}" "${RS[@]}" \
     | grep -vE '^[^:]+:[0-9]+:\s*(//|//!|/\*|\*)' \
     | grep -vE '^engine/src/runtime/telemetry\.rs:' \
     | exclude_logs)"

fail_block 20 "datos genéricos serde_json::Value/Map fuera de puertas declaradas" \
  "$(grep -rnE '\bserde_json::(Value|Map)\b|serde_json::\{[^}]*\b(Value|Map)\b' "${ROOTS[@]}" "${RS[@]}" \
     | grep -vE '^service/src/clients/[a-z0-9_/]+\.rs:' \
     | grep -vE '^service/src/http/[a-z0-9_/]+\.rs:' \
     | grep -vE '^engine/src/(runtime/telemetry|types/error|web/response/envelope)\.rs:' \
     | exclude_logs)"

fail_block 21 "trace_id fabricado en ceros, x-request-id o filter_map(... .ok()) silencioso" \
  "$( { grep -rnE '0{32}' "${ROOTS[@]}" "${RS[@]}" | grep -vE '^[^:]+:[0-9]+:\s*(//|//!|/\*|\*)'
         grep -rni 'x-request-id' "${ROOTS[@]}" "${RS[@]}"
         grep -rnE 'filter_map\(.*\.ok\(\)' "${ROOTS[@]}" "${RS[@]}"; } )"

fail_block 22 "timeout por petición en clients (anula el presupuesto de HttpClient)" \
  "$(if [[ ${#CLIENT_DIRS[@]} -gt 0 ]]; then grep -rnE '\.timeout\(' "${CLIENT_DIRS[@]}" "${RS[@]}" | grep -vE '^[^:]+:[0-9]+:\s*(//|//!|/\*|\*)'; fi)"

fail_block 23 "lectura directa de variables de entorno (env::var) fuera de engine/src/runtime/env.rs" \
  "$(grep -rnE '\b(std::)?env::var(_os)?\s*\(' "${ROOTS[@]}" "${RS[@]}" \
     | grep -vE '^engine/src/runtime/env\.rs:' \
     | grep -vE '^[^:]+:[0-9]+:\s*(//|//!|/\*|\*)' \
     | exclude_tests)"

fail_block 24 "contratos dinámicos o fugas de BSON/Collection en firmas públicas de stores/" \
  "$(store_public_leaks | exclude_tests)"

# ---------------------------------------------------------------------------
# Checks de inspección
# ---------------------------------------------------------------------------

inspect_block A "composición e inyección entre servicios (DAG en service/src/services)" \
  "$(grep -rn 'crate::services::' service/src/services "${RS[@]}")"

inspect_block B "archivos en service/src/ (espacio del microservicio)" \
  "$(find service/src -name '*.rs' -exec wc -l {} + | awk '$2 != "total" && $1 > 0' | sort -rn)"

if [[ $FAILURES -gt 0 ]]; then
  printf '\nBatería de invariantes: %d check(s) en fallo.\n' "$FAILURES"
  exit 1
fi

HARD_CHECKS="$(grep -cE '^fail_block [0-9]+ ' "$0")"
printf '\nBatería de invariantes: %d checks duros en verde.\n' "$HARD_CHECKS"
exit 0
