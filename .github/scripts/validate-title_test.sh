#!/usr/bin/env bash
set -u
cd "$(dirname "$0")"

expect() {
    local want="$1"; shift
    ./validate-title.sh "$@" > /dev/null
    local got=$?
    if [ "$got" -ne "$want" ]; then
        echo "FAIL: [$*] expected $want, got $got"
        exit 1
    fi
}

expect 2
expect 2 'feat: One' 'feat: Two'

expect 3 'chore: This title  has  everything     valid except that    its too long'
expect 3 'bot Bump github.com/alternativesourcenetwork/defradb from 1.1.0.1.0.0 to 1.1.0.1.0.1'

expect 4 'chore This title has no colon'
expect 4 'bot Bump tokio from 1.2.3 to 1.2.4'
expect 4 'fix(): Empty scope'
expect 4 'fix!(query): Bang before scope'

expect 5 'feat: a'
expect 5 'feat: '
expect 5 'feat:'

expect 6 'feat:There is no space between label & desc.'
expect 6 'fix(query):No space after scope'

expect 7 'ci: lowercase first character after label'
expect 7 'fix(query): lowercase after scope'

expect 8 'ci: Last character should not be period.'
expect 8 'ci: Last character is a space '
expect 8 'ci: Last character is a `tick`'

expect 9 'bug: This is an invalid label'
expect 9 'bug(p2p): This is an invalid label'
expect 9 'Feat: Capitalized label'

for label in chore ci docs feat fix perf refactor test tools bot; do
    expect 0 "$label: This is a valid title"
done
expect 0 'ci: Last character is a number v1.5.0'
expect 0 'ci: Last character is not lowercase alphabeT'
expect 0 'fix(query): Scoped title'
expect 0 'refactor(p2p)!: Breaking scoped title'
expect 0 'feat!: Breaking title'
expect 0 'fix(db-merge): Hyphenated scope'
expect 0 'fix(db): Path db::merge in the title'
expect 0 'bot: Bump the cargo group across 1 directory with 12 updates in total'
expect 0 'bot(deps): Bump the cargo group across 1 directory with 12 updates'

echo "PASS"
