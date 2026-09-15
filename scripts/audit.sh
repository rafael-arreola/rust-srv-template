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

# ---------------------------------------------------------------------------
# Checks duros
# ---------------------------------------------------------------------------

fail_block 1 "unwrap()/expect() en código de aplicación" \
  "$(grep -rnE '\bunwrap\s*\(|\bexpect\s*\(' src "${RS[@]}" | grep -v 'unwrap_or')"

fail_block 2 "construcción cruda de cliente reqwest fuera de core::net" \
  "$(grep -rn 'reqwest::Client::new\|reqwest::ClientBuilder' src "${RS[@]}" | grep -v '^src/core/net/')"

fail_block 3 "sintaxis de ruta axum con dos puntos (usar {id})" \
  "$(grep -rnE 'route\("[^"]*:' src "${RS[@]}")"

fail_block 4 "TimeoutLayer de tower-http (rompe el envelope uniforme)" \
  "$(grep -rn 'TimeoutLayer' src "${RS[@]}")"

fail_block 5 "persistencia privada cruzando módulos (::document::)" \
  "$(grep -rn 'crate::modules::.*::document::' src/modules "${RS[@]}")"

fail_block 6 "shared kernel importando módulos, vendors o infraestructura" \
  "$(grep -rnE 'crate::modules|crate::vendors|\bbson\b|\bmongodb\b|\baxum\b' src/shared "${RS[@]}" | grep -vE '^[^:]+:[0-9]+:\s*(//|//!)')"

fail_block 7 "core importando shared o modules" \
  "$(grep -rn 'crate::shared\|crate::modules' src/core "${RS[@]}")"

fail_block 8 "extractores crudos Query<T>/Path<T> fuera de core::http::extractors" \
  "$(grep -rnE '\b(Query|Path)<' src "${RS[@]}" | grep -v '^src/core/http/extractors/')"

fail_block 9 "archivo comodín model.rs" \
  "$(find src/modules -name 'model.rs')"

fail_block 10 "internos del slice declarados pub mod" \
  "$(grep -rnE 'pub mod (model|repository|document|entity|controller);' src/modules "${RS[@]}")"

fail_block 11 "structs Command dentro de dto" \
  "$(find src/modules \( -name 'dto.rs' -o -path '*/dto/*.rs' \) -exec grep -HnE 'struct .*Command' {} +)"

fail_block 12 "tuplas crudas devueltas por repositorios" \
  "$(grep -rnE 'AppResult<\(Vec<.*>,\s*u64\)>' src/modules "${RS[@]}")"

fail_block 13 "tracking ad-hoc x-request-id" \
  "$(grep -rni 'x-request-id' src "${RS[@]}")"

fail_block 14 "re-exports que anulan pub(super)" \
  "$(grep -rnE 'pub use (entity|repository|document|controller)::' src/modules "${RS[@]}")"

fail_block 15 "archivo de slice por encima de 400 líneas" \
  "$(find src/modules -name '*.rs' -exec wc -l {} + | awk '$1 > 400 && $2 != "total"')"

fail_block 16 "rutas totalmente calificadas en línea (los use van arriba)" \
  "$(grep -rnE '(crate::modules|crate::core|crate::vendors|crate::shared)::' src "${RS[@]}" \
     | grep -vE '^[^:]+:[0-9]+:\s*(pub )?use ' \
     | grep -vE '^[^:]+:[0-9]+:\s*(//|//!|/\*)')"

fail_block 17 "#[allow(clippy::too_many_arguments)]" \
  "$(grep -rn 'clippy::too_many_arguments' src "${RS[@]}")"

fail_block 18 "#[tracing::instrument] sin skip_all" \
  "$(grep -rnE '#\[(tracing::)?instrument' src "${RS[@]}" | grep -v 'skip_all')"

fail_block 19 "slice sin cabecera de intención (//! en la línea 1 de mod.rs)" \
  "$(for f in src/modules/*/mod.rs; do head -1 "$f" | grep -q '^//!' || echo "$f"; done)"

# ---------------------------------------------------------------------------
# Checks de inspección (salida para revisión humana, nunca fallan)
# ---------------------------------------------------------------------------

inspect_block A "imports inter-módulo: el grafo debe ser acíclico (si X importa Y, Y jamás importa X)" \
  "$(grep -rn 'crate::modules::' src/modules "${RS[@]}")"

inspect_block B "submódulos anidados: revisar regla de fachada única (máximo un nivel)" \
  "$(find src/modules -mindepth 2 -name 'mod.rs')"

inspect_block C "lecturas de extensions() en módulos: el service jamás debe leerlas" \
  "$(grep -rn 'extensions()' src/modules "${RS[@]}")"

inspect_block D "archivos de core/shared por encima de 400 líneas (umbral duro solo en slices)" \
  "$(find src/core src/shared -name '*.rs' -exec wc -l {} + | awk '$1 > 400 && $2 != "total"')"

inspect_block E "internos declarados pub(super) mod: visibles para los slices hermanos, deben ser mod privado (Guardrail 2)" \
  "$(grep -rnE 'pub\(super\) mod (entity|document|repository|controller|command)' src/modules "${RS[@]}")"

# ---------------------------------------------------------------------------

if [[ $FAILURES -gt 0 ]]; then
  printf '\nBatería de invariantes: %d check(s) en fallo.\n' "$FAILURES"
  exit 1
fi

printf '\nBatería de invariantes: 19 checks duros en verde.\n'
exit 0
