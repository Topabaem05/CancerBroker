#!/bin/sh

if [ ! -x "$PWD/session-memory-plugin" ] || [ ! -x "$PWD/scripts/bin/session-memory-plugin" ]; then
  printf '%s\n' "Run '. ./scripts/dev-env.sh' from the repository root." >&2
  return 1 2>/dev/null || exit 1
fi

case ":$PATH:" in
  *":$PWD/scripts/bin:"*)
    ;;
  *)
    export PATH="$PWD/scripts/bin:$PATH"
    ;;
esac
