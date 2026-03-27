#!/bin/bash
set -e

# Configuration: [filename]:[limit_in_kb]
BUDGETS=(
    "navin_token.wasm:25"
    "shipment.wasm:175"
)

WASM_DIR="target/wasm32-unknown-unknown/release"
EXIT_CODE=0

echo "Checking WASM size budget..."
echo "---------------------------"

for entry in "${BUDGETS[@]}"; do
    FILENAME="${entry%%:*}"
    LIMIT_KB="${entry##*:}"
    FILEPATH="$WASM_DIR/$FILENAME"

    if [ ! -f "$FILEPATH" ]; then
        echo "❌ Error: $FILENAME not found in $WASM_DIR"
        EXIT_CODE=1
        continue
    fi

    # Get size in bytes
    SIZE_BYTES=$(stat -f%z "$FILEPATH" 2>/dev/null || stat -c%s "$FILEPATH")
    SIZE_KB=$((SIZE_BYTES / 1024))

    if [ "$SIZE_KB" -gt "$LIMIT_KB" ]; then
        echo "❌ $FILENAME: ${SIZE_KB}KB (Limit: ${LIMIT_KB}KB) - BUDGET EXCEEDED"
        EXIT_CODE=1
    else
        echo "✅ $FILENAME: ${SIZE_KB}KB (Limit: ${LIMIT_KB}KB) - Within budget"
    fi
done

echo "---------------------------"
if [ $EXIT_CODE -eq 0 ]; then
    echo "✓ All contracts are within their size budget."
else
    echo "FAILED: Some contracts exceeded their size budget."
fi

exit $EXIT_CODE
