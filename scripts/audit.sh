#!/usr/bin/env bash
# Batería de invariantes arquitectónicos.
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
# El check 1 ignora los bloques `#[cfg(test)]` porque `clippy.toml` activa
# `allow-unwrap-in-tests` / `allow-expect-in-tests` y clippy ya los admite ahí.
# El resto de los checks cubre todo el árbol.

set -uo pipefail

cd "$(dirname "$0")/.." || exit 1

QUIET=0
[[ "${1:-}" == "--quiet" ]] && QUIET=1

FAILURES=0
RS=('--include=*.rs')

# `grep` sale con 1 cuando no hay coincidencias y con 2 cuando falla (regex inválida, flag que la
# implementación local no soporta: ugrep en macOS, GNU grep en la imagen). Sin este envoltorio un
# grep roto no imprime nada y su check se reporta `[ ok ]`. Un fallo imprime un marcador que
# atraviesa los filtros del pipeline y deja el check en `[FAIL]`. Las funciones se heredan en cada
# `$(...)`; lo que no cubre es `find -exec grep` o `xargs grep`, que invocan el binario
# directamente: un check que recorre archivos con `find` itera en un `while read` para heredarla.
# `command grep` marca los sitios donde un exit 2 es esperado (glob opcional sin coincidencias).

# Un clon puede no tener `src/vendors` (o ningún vendor): los checks lo reciben como directorio
# opcional para no convertir su ausencia en un exit 2 del grep.
shopt -s nullglob
VENDOR_DIRS=()
[[ -d src/vendors ]] && VENDOR_DIRS=(src/vendors)
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
  "$(grep -rnE 'reqwest::Client(::new|::builder|Builder)|\bClient::(new|builder)\(' src "${RS[@]}" | grep -v '^src/core/net/')"

fail_block 3 "sintaxis de ruta axum con dos puntos (usar {id})" \
  "$(grep -rnE 'route\("[^"]*:' src "${RS[@]}")"

fail_block 4 "TimeoutLayer de tower-http (rompe el envelope uniforme)" \
  "$(grep -rn 'TimeoutLayer' src "${RS[@]}")"

fail_block 5 "persistencia privada cruzando módulos (::document:: / ::entity:: / ::repository::)" \
  "$(grep -rnE 'crate::modules::[a-z_]+::(document|entity|repository)\b' src/modules "${RS[@]}" \
     | grep -vE '^src/modules/([a-z_]+)/.*:[0-9]+:.*crate::modules::\1::')"

fail_block 6 "shared kernel importando módulos, vendors o infraestructura" \
  "$(grep -rnE 'crate::modules|crate::vendors|\bbson\b|\bmongodb\b|\baxum\b' src/shared "${RS[@]}" | grep -vE '^[^:]+:[0-9]+:\s*(//|//!)')"

fail_block 7 "core importando shared, modules o vendors" \
  "$(grep -rn 'crate::shared\|crate::modules\|crate::vendors' src/core "${RS[@]}")"

fail_block 8 "extractores crudos Query<T>/Path<T> fuera de core::http::extractors" \
  "$(grep -rnE '\b(Query|Path)<' src "${RS[@]}" | grep -v '^src/core/http/extractors/')"

fail_block 9 "archivo comodín model.rs" \
  "$(find src/modules -name 'model.rs')"

fail_block 10 "internos del slice declarados pub mod (incluye pub(crate)/pub(super)/pub(in ...))" \
  "$(grep -rnE 'pub(\([a-z: ]+\))? mod (model|repository|document|entity|controller|command|pipeline|strategies|documents|defaults|errors|criteria|consumer);' src/modules "${RS[@]}")"

command_structs_in_dto() {
  find src/modules \( -name 'dto.rs' -o -path '*/dto/*.rs' \) -print0 \
    | while IFS= read -r -d '' file; do grep -HnE 'struct .*Command' "$file"; done
}
fail_block 11 "structs Command dentro de dto" \
  "$(command_structs_in_dto)"

fail_block 12 "tuplas crudas devueltas por repositorios" \
  "$(grep -rnE 'AppResult<\(Vec<.*>,\s*[ui](8|16|32|64|size)\)>' src/modules "${RS[@]}")"

fail_block 13 "tracking ad-hoc x-request-id" \
  "$(grep -rni 'x-request-id' src "${RS[@]}")"

fail_block 14 "re-exports de internos que anulan la frontera del slice" \
  "$(grep -rnE 'pub(\([a-z: ]+\))? use .*\b(entity|repository|document|controller|command|pipeline)::' src/modules "${RS[@]}")"

fail_block 16 "rutas totalmente calificadas en línea (los use van arriba)" \
  "$(grep -rnE '(crate::modules|crate::core|crate::vendors|crate::shared)::' src "${RS[@]}" \
     | grep -vE '^[^:]+:[0-9]+:\s*(pub )?use ' \
     | grep -vE '^[^:]+:[0-9]+:\s*(//|//!|/\*)')"

fail_block 17 "#[allow(clippy::too_many_arguments)]" \
  "$(grep -rn 'clippy::too_many_arguments' src "${RS[@]}")"

fail_block 18 "#[tracing::instrument] sin skip_all" \
  "$(instrument_without_skip_all)"

fail_block 19 "slice sin cabecera de intención (//! en la línea 1 de mod.rs y una línea //! **Intent:**)" \
  "$(for f in src/modules/*/mod.rs; do
       head -1 "$f" | grep -q '^//!' && grep -q '^//! \*\*Intent:\*\*' "$f" || echo "$f"
     done)"

fail_block 20 "internos declarados pub(super)/pub(crate)/pub(in ...) mod: visibles fuera del slice" \
  "$(grep -rnE 'pub\((super|crate|in [a-z:]+)\) mod ' src/modules "${RS[@]}")"

# Solo en los slices que guardan `deleted_at` en su documento un borrado físico rompe el
# soft-delete; un slice sin `deleted_at` puede borrar.
physical_delete_in_soft_delete_slices() {
  for slice in src/modules/*/; do
    slice="${slice%/}"
    # `command grep`: que un slice no tenga `document.rs` (exit 2 con -s) es esperado aquí.
    if command grep -qsE '\bdeleted_at\b' "$slice"/document.rs "$slice"/document/*.rs 2>/dev/null; then
      grep -rn 'delete_one\|delete_many\|find_one_and_delete' "$slice" "${RS[@]}" \
        | grep -vE '^[^:]+:[0-9]+:\s*(//|//!|/\*|\*)'
    fi
  done
}

fail_block 21 "borrado físico en slices que eligieron soft-delete (deleted_at en su document)" \
  "$(physical_delete_in_soft_delete_slices)"

fail_block 22 "#[allow(clippy::should_implement_trait)]: usar FromStr/TryFrom<&str>" \
  "$(grep -rn 'clippy::should_implement_trait' src "${RS[@]}")"

# Los nombres de entidad se leen de los `entity.rs`/`entity/*.rs` del propio repositorio, así el
# check sigue a las entidades reales en lugar de a una lista escrita a mano. La firma se reconstruye
# hasta su `{` porque rustfmt parte las largas y deja `-> AppResult<Order>` en otra línea. Cubre
# toda superficie pública entre slices: service, workflow y contract.
# `command grep`: la forma de directorio `entity/` es opcional y su glob puede no existir.
ENTITY_NAMES="$(command grep -hoE 'pub (struct|enum) [A-Z][A-Za-z0-9]*' src/modules/*/entity.rs src/modules/*/entity/*.rs 2>/dev/null \
  | awk '{print $3}' | sort -u | paste -sd'|' -)"
public_signatures_with_entities() {
  find src/modules \( -name 'service.rs' -o -name 'workflow.rs' -o -name 'contract.rs' \) -print0 \
    | xargs -0 awk -v names="${ENTITY_NAMES:-__none__}" '
      /^[[:space:]]*pub(\([a-z: ]+\))? (async )?fn / {
        sig = $0; start = FNR
        while (sig !~ /[{;][[:space:]]*$/ && (getline > 0)) { sig = sig " " $0 }
        gsub(/[ \t]+/, " ", sig)
        # Fronteras explícitas: `\<`/`\>` no existen en el awk de macOS ni en mawk.
        if (sig ~ ("(^|[^A-Za-z0-9_])(" names "|[A-Za-z0-9]+Document)([^A-Za-z0-9_]|$)")) print FILENAME ":" start ": " sig
      }
    '
}
fail_block 23 "entidades o documentos en la firma pública de un service/workflow/contract" \
  "$(public_signatures_with_entities)"

fail_block 24 "construcción de payloads con json! fuera de la superficie de logging" \
  "$(grep -rnE '\bjson!\s*\(' src "${RS[@]}" \
     | grep -vE '^[^:]+:[0-9]+:\s*(//|//!|/\*|\*)' \
     | grep -vE '^src/core/telemetry/' \
     | exclude_logs)"

fail_block 25 "datos genéricos serde_json::Value/Map fuera de las puertas declaradas" \
  "$(grep -rnE '\bserde_json::(Value|Map)\b|serde_json::\{[^}]*\b(Value|Map)\b' src "${RS[@]}" \
     | grep -vE '^src/vendors/[a-z0-9_]+/dto(/[a-z0-9_]+\.rs|\.rs):' \
     | grep -vE '^src/modules/[a-z0-9_]+/dto/raw_payload\.rs:' \
     | grep -vE '^src/core/telemetry/' \
     | exclude_logs)"

fail_block 26 "acceso a campos de payload por cadena en modules/vendors (.get(\"...\"), [\"...\"], .as_array()/.as_object()/.as_u64()...)" \
  "$( { grep -rnE '\.get(_mut)?\(\s*\"|\[\s*\"[A-Za-z0-9_]+\"\s*\]|\.as_(array|object)(_mut)?\(\)|\.as_(u64|i64|f64|bool)\(\)' src/modules "${RS[@]}"
         # En vendors `.get("…")` es también la llamada HTTP (`client.get("https://…")`): ahí solo
         # cuentan el acceso por índice y los `.as_*()`, que son exclusivos de `Value`.
         if [[ ${#VENDOR_DIRS[@]} -gt 0 ]]; then
           grep -rnE '\[\s*\"[A-Za-z0-9_]+\"\s*\]|\.as_(array|object)(_mut)?\(\)|\.as_(u64|i64|f64|bool)\(\)' "${VENDOR_DIRS[@]}" "${RS[@]}"
         fi; } | exclude_logs)"

# En `modules` y `shared` no hay magnitudes en punto flotante. El dinero es entero de
# unidades menores + `Currency`; pesos, porcentajes o coordenadas se modelan como enteros escalados.
# Si un slice real necesita `f64` para una magnitud que no es dinero, la excepción se declara aquí
# con su ruta y su motivo, no borrando el check.
fail_block 27 "punto flotante (f32/f64) en modules/shared: el dinero es enteros de unidades menores + Currency" \
  "$(grep -rnE '\b(f32|f64)\b' src/modules src/shared "${RS[@]}" | grep -vE '^[^:]+:[0-9]+:\s*(//|//!|/\*|\*)')"

# Un `trace_id` en ceros es inválido según W3C. Un envelope fuera de span es un defecto de
# cableado que se reporta en el log, nunca un identificador fabricado.
fail_block 28 "trace_id fabricado en ceros" \
  "$(grep -rnE '0{32}' src "${RS[@]}" | grep -vE '^[^:]+:[0-9]+:\s*(//|//!|/\*|\*)')"

# Descartar en silencio las entradas que no parsean (`filter_map(|s| s.parse().ok())`)
# es un fallback silencioso: la configuración inválida desaparece sin error ni log.
fail_block 29 "descarte silencioso de valores inválidos con filter_map(... .ok())" \
  "$(grep -rnE 'filter_map\(.*\.ok\(\)' src "${RS[@]}")"

# Matriz de aristas: vendors no importan modules ni otros vendors, config no importa
# nada del árbol de dominio, shared solo importa `core::error`. Los checks 6 y 7 cubren el resto.
dependency_edge_violations() {
  grep -rn 'crate::modules' "${VENDOR_DIRS[@]}" src/config.rs "${RS[@]}"
  grep -rn 'crate::vendors\|crate::shared' src/config.rs
  for vendor in src/vendors/*/; do
    vendor="$(basename "$vendor")"
    grep -rnE 'crate::vendors::[a-z0-9_]+' "src/vendors/$vendor" "${RS[@]}" \
      | grep -v "crate::vendors::$vendor\b"
  done
  grep -rn 'crate::core::' src/shared "${RS[@]}" | grep -v 'crate::core::error'
}
fail_block 30 "aristas prohibidas: vendors→modules, vendor→vendor, config→dominio, shared→core no-error" \
  "$(dependency_edge_violations | grep -vE '^[^:]+:[0-9]+:\s*(//|//!|/\*|\*)')"

# El tope de 10 s de `HttpClient` se anula con un `.timeout(...)` por petición
# (reqwest: "overrides the timeout configured using ClientBuilder::timeout()"). Un vendor que
# necesite otro presupuesto lo pide a `core::net`, no lo fija en su cliente.
fail_block 31 "timeout por petición en vendors: anula el tope de HttpClient" \
  "$(if [[ ${#VENDOR_DIRS[@]} -gt 0 ]]; then grep -rnE '\.timeout\(' "${VENDOR_DIRS[@]}" "${RS[@]}" | grep -vE '^[^:]+:[0-9]+:\s*(//|//!|/\*|\*)'; fi)"

# El driver es un detalle de almacenamiento. En un slice solo `repository` y
# `document` (archivo o directorio) nombran `mongodb`/`bson` o bajan al handle con `.mongo()`; el
# resto del slice, incluido `entity`, `dto` y `mod.rs`, habla en tipos de dominio y `Store`.
fail_block 32 "driver de persistencia (mongodb/bson/.mongo()) fuera de repository/document en un slice" \
  "$(grep -rnE '(^|[^A-Za-z0-9_])(mongodb|bson)::|\buse (mongodb|bson)\b|\.mongo\(\)' src/modules "${RS[@]}" \
     | grep -vE '^src/modules/[a-z0-9_]+/(repository|document)(\.rs|/[a-z0-9_]+\.rs):' \
     | grep -vE '^[^:]+:[0-9]+:\s*(//|//!|/\*|\*)')"

# Cloud Run resta CPU al proceso cuando la respuesta sale, así que una tarea `spawn`
# que debe terminar puede quedarse congelada. El trabajo se hace en línea; los consumidores de
# larga vida se lanzan en el composition root o en `core`, no dentro de un slice o un vendor.
fail_block 33 "tokio::spawn en modules/vendors: trabajo que debe terminar va en línea" \
  "$(grep -rnE '\btokio::(task::)?spawn\b|\bspawn\(' src/modules "${VENDOR_DIRS[@]}" "${RS[@]}" \
     | grep -vE '^[^:]+:[0-9]+:\s*(//|//!|/\*|\*)')"

# Entity y document representan lo mismo con tipos distintos. La entidad es dominio
# puro (`String`, `chrono`, enums sin serde); el mapeo a la forma almacenada vive solo en `document`.
# (`bson`/`mongodb` en una entity ya lo cubre el check 32.)
files_named() {
  find src/modules \( -name "$1.rs" -o -path "*/$1/*.rs" \) -print0
}
entity_files_with_serde() {
  files_named entity | while IFS= read -r -d '' file; do grep -Hn '\bserde\b' "$file"; done
}
fail_block 34 "serde en una entity: la forma almacenada o serializada vive en document/dto" \
  "$(entity_files_with_serde | grep -vE '^[^:]+:[0-9]+:\s*(//|//!|/\*|\*)')"

# `_id` es estrictamente `ObjectId`. El campo que sigue a `rename = "_id"` con otro
# tipo (String, ModelId, Option<String>...) es la puerta al `$or` polimórfico que anula IDHACK.
document_id_not_objectid() {
  files_named document | xargs -0 -n1 awk '
    /rename = "_id"/ { want = 1; next }
    want && /pub [a-z_]+:/ { if ($0 !~ /ObjectId/) print FILENAME ":" FNR ": " $0; want = 0 }
  '
}
fail_block 35 "_id de un document que no es ObjectId" \
  "$(document_id_not_objectid)"

# El reloj es un efecto. El service lee `Utc::now()` una vez por operación y lo pasa a
# la entidad y al repositorio; así la entidad es determinista y una operación usa un solo instante.
clock_reads_outside_service() {
  for kind in entity document repository; do
    files_named "$kind" | while IFS= read -r -d '' file; do
      grep -HnE '\b(Utc|Local|DateTime|SystemTime|Instant)::now\(' "$file"
    done
  done
}
fail_block 36 "lectura del reloj en entity/document/repository: el instante lo pasa el service" \
  "$(clock_reads_outside_service | grep -vE '^[^:]+:[0-9]+:\s*(//|//!|/\*|\*)')"

# ---------------------------------------------------------------------------
# Checks de inspección (salida para revisión humana, nunca fallan)
# ---------------------------------------------------------------------------

inspect_block A "imports inter-módulo: el grafo debe ser acíclico (si X importa Y, Y jamás importa X)" \
  "$(grep -rn 'crate::modules::' src/modules "${RS[@]}" \
     | grep -vE '^src/modules/([a-z_]+)/.*:[0-9]+:.*crate::modules::\1::')"

inspect_block B "submódulos anidados: revisar regla de fachada única (máximo un nivel)" \
  "$(find src/modules -mindepth 3 -name 'mod.rs' | grep -vE '/(dto|entity|document|repository|strategies)/mod\.rs$')"

inspect_block C "lecturas de extensions() en módulos: el service jamás debe leerlas" \
  "$(grep -rn 'extensions()' src/modules "${RS[@]}")"

inspect_block D "archivos más grandes de src (sin umbral: revisar que cada uno cubra una sola familia de actividades)" \
  "$(find src -name '*.rs' -exec wc -l {} + | awk '$2 != "total" && $1 > 0' | sort -rn | head -20)"

inspect_block E "Paginated<T> cruzando la frontera del repositorio (debe ser QueryResult<T>)" \
  "$(grep -rn 'Paginated' src/modules --include='repository.rs' --include='repository/*.rs')"

inspect_block F "lifetimes explícitos (sin 'de ni for<'a>): cada coincidencia debe justificar que no hay alternativa" \
  "$(grep -rnE "<'[a-z]" src "${RS[@]}" | grep -vE "<'de\b|for<'" | grep -vE '^[^:]+:[0-9]+:\s*(//|//!|/\*|\*)')"

# Decisiones que el template deja a quien lo clona: identidad del template en los manifiestos de
# despliegue. Inspección, no fallo: en este repositorio es el estado esperado; en un clon es la
# lista de lo que aún no se decidió.
inspect_block H "decisiones de clonado pendientes: identidad 'rust-srv-template' en Cargo.toml/cloudbuild" \
  "$(grep -nE 'rust-srv-template|service-name-db|service-account@' Cargo.toml build/cloudbuild.yaml)"

# ---------------------------------------------------------------------------

if [[ $FAILURES -gt 0 ]]; then
  printf '\nBatería de invariantes: %d check(s) en fallo.\n' "$FAILURES"
  exit 1
fi

HARD_CHECKS="$(grep -cE '^fail_block [0-9]+ ' "$0")"
printf '\nBatería de invariantes: %d checks duros en verde.\n' "$HARD_CHECKS"
exit 0
