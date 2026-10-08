#!/usr/bin/env bash
# Validates a PR title against the conventional-commit subset shared with the Go
# repo (tools/scripts/validate-conventional-style.sh there), extended with an
# optional `(scope)` and breaking-change `!`: `fix(query)!: Capitalized summary`.
# Exit codes match the Go script so failures read the same in both repos.

# Character ranges below must mean ASCII, not the locale's collation order.
export LC_COLLATE=C

readonly BOT_LABEL="bot"
readonly -a VALID_LABELS=(chore ci docs feat fix perf refactor test tools "${BOT_LABEL}")
readonly HEADER_RE='^([^:(!]+)(\([^()]+\))?(!)?:(.*)$'

if [ "$#" -ne 1 ]; then
    echo "Error: Invalid number of arguments (pass title as 1 string argument)."
    exit 2
fi

readonly TITLE="$1"

if [[ "$TITLE" =~ ^"$BOT_LABEL"(\([^()]+\))?: ]]; then
    echo "Info: Title is from a bot, skipping length-related title validation."
elif [ "${#TITLE}" -gt 60 ]; then
    echo "Error: The length of the title is too long (should be 60 or less)."
    exit 3
fi

if ! [[ "$TITLE" =~ $HEADER_RE ]]; then
    echo "Error: Title does not start with 'label:', 'label(scope):' or 'label(scope)!:'."
    exit 4
fi

readonly LABEL="${BASH_REMATCH[1]}"
readonly DESCRIPTION="${BASH_REMATCH[4]}"

echo "Info: label = [$LABEL] scope = [${BASH_REMATCH[2]}] breaking = [${BASH_REMATCH[3]}]"
echo "Info: description = [$DESCRIPTION]"

if [ "${#DESCRIPTION}" -le 2 ]; then
    echo "Error: Description is too short."
    exit 5
fi

if [ "${DESCRIPTION:0:1}" != " " ]; then
    echo "Error: There is no space between label and description."
    exit 6
fi

if [[ "${DESCRIPTION:1:1}" != [A-Z] ]]; then
    echo "Error: First character after the label is not an uppercase alphabet."
    exit 7
fi

if [[ "${DESCRIPTION: -1}" != [a-zA-Z0-9] ]]; then
    echo "Error: Last character is an invalid character."
    exit 8
fi

for valid in "${VALID_LABELS[@]}"; do
    if [ "$LABEL" == "$valid" ]; then
        echo "Success: Title's label and description style is valid."
        exit 0
    fi
done

echo "Error: The label used in the title isn't a valid label (${VALID_LABELS[*]})."
exit 9
