#!/usr/bin/env bash
# Batería de invariantes arquitectónicos (AGENTS.md §3.2).
#
# Checks duros: cualquier coincidencia es una violación y aborta con código 1.
# Checks de inspección: imprimen su salida para revisión humana y nunca fallan el build.
#
# Uso:
#   scripts/audit.sh            # ejecuta todo
#   scripts/audit.sh --quiet    # solo reporta violaciones
#
# Se ejecuta en la etapa builder de build/Dockerfile: si la batería falla, no se
# produce imagen y el deploy no puede ocurrir.
#
# Desviación respecto a la plantilla: el check 1 ignora los bloques `#[cfg(test)]`
# porque `clippy.toml` ya declara `allow-unwrap-in-tests` / `allow-expect-in-tests`
# como política del proyecto. El resto de los checks cubre todo el árbol.

set -uo pipefail

cd "$(dirname "$0")/.." || exit 1

QUIET=0
[[ "${1:-}" == "--quiet" ]] && QUIET=1

FAILURES=0
RS=(--include=*.rs)

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

# Índice "archivo:línea" de cada línea contenida en un bloque `#[cfg(test)]` de nivel raíz.
TEST_INDEX="$(mktemp "${TMPDIR:-/tmp}/audit_test_lines.XXXXXX")"
trap 'rm -f "$TEST_INDEX" "$LOG_INDEX"' EXIT

find src -name '*.rs' -print0 | xargs -0 awk '
  FNR == 1 { intest = 0 }
  /^#\[cfg\(test\)\]/ { intest = 1 }
  intest { print FILENAME ":" FNR }
  intest && /^}/ { intest = 0 }
' > "$TEST_INDEX"

# Filtra de una salida `grep -rn` las coincidencias que caen dentro de código de test.
exclude_tests() {
  awk -v idx="$TEST_INDEX" '
    BEGIN { while ((getline line < idx) > 0) skip[line] = 1 }
    { split($0, p, ":"); if (!((p[1] ":" p[2]) in skip)) print }
  '
}

# Índice "archivo:línea" de cada línea dentro de una llamada de logging (`tracing::`/`log::`).
# Los logs son la superficie donde un payload sin tipo es admisible: son observabilidad, no
# contrato. Una sentencia de log termina en `;` o en la llave que cierra su bloque; el tope de
# líneas acota el arrastre si el terminador no aparece, y al cerrar antes solo puede volver el
# check más estricto, nunca más laxo.
LOG_INDEX="$(mktemp "${TMPDIR:-/tmp}/audit_log_lines.XXXXXX")"

find src -name '*.rs' -print0 | xargs -0 awk -v cap=60 '
  FNR == 1 { inlog = 0; run = 0 }
  !inlog && /(tracing|log)::(trace|debug|info|warn|error|event)![(]/ { inlog = 1; run = 0 }
  inlog {
    print FILENAME ":" FNR
    run++
    if ($0 ~ /;[[:space:]]*$/ || $0 ~ /^[[:space:]]*}[[:space:]]*$/ || run > cap) inlog = 0
  }
' > "$LOG_INDEX"

# Filtra de una salida `grep -rn` las coincidencias que caen dentro de una llamada de logging.
exclude_logs() {
  awk -v idx="$LOG_INDEX" '
    BEGIN { while ((getline line < idx) > 0) skip[line] = 1 }
    { split($0, p, ":"); if (!((p[1] ":" p[2]) in skip)) print }
  '
}

# Reconstruye atributos `#[tracing::instrument(...)]` multilínea y reporta los que no llevan skip_all.
instrument_without_skip_all() {
  find src -name '*.rs' -print0 | xargs -0 awk '
    /#\[(tracing::)?instrument/ {
      buf = $0; start = FNR
      while (buf !~ /\)\]/ && buf !~ /instrument\]/ && (getline > 0)) { buf = buf " " $0 }
      gsub(/[ \t]+/, " ", buf)
      if (buf !~ /skip_all/) print FILENAME ":" start ": " buf
    }
  '
}

# ---------------------------------------------------------------------------
# Checks duros
# ---------------------------------------------------------------------------

fail_block 1 "unwrap()/expect() en código de aplicación (tests exentos vía clippy.toml)" \
  "$(grep -rnE '\bunwrap\s*\(|\bexpect\s*\(' src "${RS[@]}" | grep -v 'unwrap_or' | exclude_tests)"

fail_block 2 "construcción cruda de cliente reqwest fuera de core::net" \
  "$(grep -rn 'reqwest::Client::new\|reqwest::ClientBuilder' src "${RS[@]}" | grep -v '^src/core/net/')"

fail_block 3 "sintaxis de ruta axum con dos puntos (usar {id})" \
  "$(grep -rnE 'route\("[^"]*:' src "${RS[@]}")"

fail_block 4 "TimeoutLayer de tower-http (rompe el envelope uniforme)" \
  "$(grep -rn 'TimeoutLayer' src "${RS[@]}")"

fail_block 5 "persistencia privada cruzando módulos (::document:: / ::entity:: / ::repository::)" \
  "$(grep -rnE 'crate::modules::[a-z_]+::(document|entity|repository)::' src/modules "${RS[@]}" \
     | grep -vE '^src/modules/([a-z_]+)/.*:[0-9]+:.*crate::modules::\1::')"

fail_block 6 "shared kernel importando módulos, vendors o infraestructura" \
  "$(grep -rnE 'crate::modules|crate::vendors|\bbson\b|\bmongodb\b|\baxum\b' src/shared "${RS[@]}" | grep -vE '^[^:]+:[0-9]+:\s*(//|//!)')"

fail_block 7 "core importando shared, modules o vendors" \
  "$(grep -rn 'crate::shared\|crate::modules\|crate::vendors' src/core "${RS[@]}")"

fail_block 8 "extractores crudos Query<T>/Path<T> fuera de core::http::extractors" \
  "$(grep -rnE '\b(Query|Path)<' src "${RS[@]}" | grep -v '^src/core/http/extractors/')"

fail_block 9 "archivo comodín model.rs" \
  "$(find src/modules -name 'model.rs')"

fail_block 10 "internos del slice declarados pub mod" \
  "$(grep -rnE 'pub mod (model|repository|document|entity|controller|command|pipeline|strategies|documents|defaults|errors);' src/modules "${RS[@]}")"

fail_block 11 "structs Command dentro de dto" \
  "$(find src/modules \( -name 'dto.rs' -o -path '*/dto/*.rs' \) -exec grep -HnE 'struct .*Command' {} +)"

fail_block 12 "tuplas crudas devueltas por repositorios" \
  "$(grep -rnE 'AppResult<\(Vec<.*>,\s*u64\)>' src/modules "${RS[@]}")"

fail_block 13 "tracking ad-hoc x-request-id" \
  "$(grep -rni 'x-request-id' src "${RS[@]}")"

fail_block 14 "re-exports de internos que anulan la frontera del slice" \
  "$(grep -rnE 'pub use ([a-z_:]+::)?(entity|repository|document|controller|command|pipeline)::' src/modules "${RS[@]}")"

fail_block 15 "archivo de slice por encima de 400 líneas" \
  "$(find src/modules -name '*.rs' -exec wc -l {} + | awk '$1 > 400 && $2 != "total"')"

fail_block 16 "rutas totalmente calificadas en línea (los use van arriba)" \
  "$(grep -rnE '(crate::modules|crate::core|crate::vendors|crate::shared)::' src "${RS[@]}" \
     | grep -vE '^[^:]+:[0-9]+:\s*(pub )?use ' \
     | grep -vE '^[^:]+:[0-9]+:\s*(//|//!|/\*)')"

fail_block 17 "#[allow(clippy::too_many_arguments)]" \
  "$(grep -rn 'clippy::too_many_arguments' src "${RS[@]}")"

fail_block 18 "#[tracing::instrument] sin skip_all" \
  "$(instrument_without_skip_all)"

fail_block 19 "slice sin cabecera de intención (//! en la línea 1 de mod.rs)" \
  "$(for f in src/modules/*/mod.rs; do head -1 "$f" | grep -q '^//!' || echo "$f"; done)"

fail_block 20 "internos declarados pub(super) mod: visibles para los slices hermanos (Guardrail 2)" \
  "$(grep -rnE 'pub\(super\) mod ' src/modules "${RS[@]}")"

fail_block 21 "borrado físico en slices con soft-delete (deleted_at)" \
  "$(grep -rn 'delete_one\|delete_many\|find_one_and_delete' src/modules "${RS[@]}" \
     | grep -vE '^[^:]+:[0-9]+:\s*(//|//!|/\*|\*)')"

fail_block 22 "#[allow(clippy::should_implement_trait)]: usar FromStr/TryFrom<&str>" \
  "$(grep -rn 'clippy::should_implement_trait' src "${RS[@]}")"

SERVICE_FILES="$(grep -rlE 'impl [A-Za-z0-9]*Service\b' src/modules "${RS[@]}" 2>/dev/null)"
fail_block 23 "entidades o documentos en la firma pública de un service" \
  "$(if [[ -n "$SERVICE_FILES" ]]; then grep -nE 'pub (async )?fn .*\b(Order|Manifest|Transport|Warehouse|Inventory|CustomState|StandardizationRule|ClientEntity|[A-Za-z0-9]+Document)\b' $SERVICE_FILES 2>/dev/null; fi)"

fail_block 24 "construcción de payloads con json! fuera de logs (usar structs con serde)" \
  "$(grep -rnE '\bjson!\s*\(' src "${RS[@]}" | grep -vE '^[^:]+:[0-9]+:\s*(//|//!|/\*|\*)' | exclude_logs)"

fail_block 25 "datos genéricos serde_json::Value/Map fuera de las puertas declaradas" \
  "$(grep -rnE '\bserde_json::(Value|Map)\b' src "${RS[@]}" \
     | grep -vE '^src/vendors/[a-z0-9_]+/dto(/[a-z0-9_]+\.rs|\.rs):' \
     | grep -vE '^src/modules/[a-z0-9_]+/dto/raw_payload\.rs:' \
     | grep -vE '^src/core/telemetry/' \
     | exclude_logs)"

fail_block 26 "acceso a campos de payload por cadena en módulos (.get(\"...\")/.as_array()/.as_u64())" \
  "$(grep -rnE '\.get(_mut)?\(\s*\"|\.as_array(_mut)?\(\)|\.as_u64\(\)' src/modules "${RS[@]}" | exclude_logs)"

# ---------------------------------------------------------------------------
# Checks de inspección (salida para revisión humana, nunca fallan)
# ---------------------------------------------------------------------------

inspect_block A "imports inter-módulo: el grafo debe ser acíclico (si X importa Y, Y jamás importa X)" \
  "$(grep -rn 'crate::modules::' src/modules "${RS[@]}" \
     | grep -vE '^src/modules/([a-z_]+)/.*:[0-9]+:.*crate::modules::\1::')"

inspect_block B "submódulos anidados: revisar regla de fachada única (máximo un nivel)" \
  "$(find src/modules -mindepth 2 -name 'mod.rs')"

inspect_block C "lecturas de extensions() en módulos: el service jamás debe leerlas" \
  "$(grep -rn 'extensions()' src/modules "${RS[@]}")"

inspect_block D "archivos de core/shared/vendors por encima de 400 líneas (umbral duro solo en slices)" \
  "$(find src/core src/shared src/vendors -name '*.rs' -exec wc -l {} + | awk '$1 > 400 && $2 != "total"')"

inspect_block E "Paginated<T> cruzando la frontera del repositorio (debe ser QueryResult<T>)" \
  "$(grep -rn 'Paginated' src/modules --include='repository.rs' --include='repository/*.rs')"

# ---------------------------------------------------------------------------

if [[ $FAILURES -gt 0 ]]; then
  printf '\nBatería de invariantes: %d check(s) en fallo.\n' "$FAILURES"
  exit 1
fi

printf '\nBatería de invariantes: 26 checks duros en verde.\n'
exit 0
