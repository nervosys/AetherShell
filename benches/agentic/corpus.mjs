// The task corpus: ten queries over 500 real GitHub issues, expressed once per
// engine, plus the oracle each answer is checked against.
//
// Every command in this file was run to completion and checked against the
// oracle before it was written down. Where an engine needed several attempts
// to reach a correct command, `ATTEMPTS` records how many -- see the caveat in
// README.md about what that number can and cannot support.

export const ENGINES = {
    aethershell: { bin: 'ae', argv: (c) => ['-c', c], label: 'AetherShell 12.0.2' },
    'bash+jq': { bin: 'bash', argv: (c) => ['-c', c], label: 'bash 5.2 + jq 1.7.1' },
    'bash+coreutils': { bin: 'bash', argv: (c) => ['-c', c], label: 'bash 5.2 + coreutils (files)' },
    sqlite: { bin: 'sqlite3', argv: (c) => ['issues.db', c], label: 'sqlite3' },
    pwsh: { bin: 'pwsh', argv: (c) => ['-NoProfile', '-Command', c], label: 'PowerShell 7.6.6' },
    nushell: { bin: 'nu', argv: (c) => ['-n', '-c', c], label: 'nushell 0.115.1' },
};

// Oracle values, computed independently in oracle.mjs from the same records.
export const ORACLE = {
    q1: '10', q2: '293', q3: '430', q4: 'dependabot[bot] 72',
    q5: '14086,14089,14090,14107,14223,14256,14309,14386,14389,14420',
    q6: '43', q7: '19', q8: '97', q9: '25', q10: '4.97',
};

// Normalise an engine's rendering of the same answer before comparing: the
// benchmark asks whether the agent got the fact, not whether the shell printed
// it with brackets, a pipe separator or a trailing newline.
export const normalise = (s) =>
    s.trim().replace(/\r/g, '').replace(/^\[|\]$/g, '')
        .replace(/[|\s]+/g, ' ').replace(/,\s+/g, ',').trim();

const AE = {
    q1: 'cat("issues.json") | from_json | where(fn(r) => r.state == "open" && contains(lower(r.title + " " + r.body), "security")) | len',
    q2: 'cat("issues.json") | from_json | where(fn(r) => r.is_pr) | len',
    q3: 'cat("issues.json") | from_json | where(fn(r) => r.state == "closed") | map(fn(r) => r.comments) | sum',
    q4: 'cat("issues.json") | from_json | where(fn(r) => r.is_pr) | group_by("user") | sort_by("Count") | last | fn(g) => g.Name + " " + to_string(g.Count)',
    q5: 'cat("issues.json") | from_json | where(fn(r) => !r.is_pr && r.state == "open" && r.comments > 5) | map(fn(r) => r.number) | sort',
    q6: 'cat("issues.json") | from_json | map(fn(r) => r.labels) | flatten | unique | len',
    q7: 'cat("issues.json") | from_json | where(fn(r) => r.state == "open" && any(r.labels, fn(l) => l == "bug")) | len',
    q8: 'cat("issues.json") | from_json | map(fn(r) => r.comments) | max',
    q9: 'let iss = cat("issues.json") | from_json | where(fn(r) => !r.is_pr) | map(fn(r) => r.user) | unique\ncat("issues.json") | from_json | where(fn(r) => r.is_pr) | map(fn(r) => r.user) | unique | where(fn(u) => any(iss, fn(i) => i == u)) | len',
    q10: 'cat("issues.json") | from_json | where(fn(r) => !r.is_pr && r.state == "open") | map(fn(r) => r.comments) | mean | round(2)',
};

const JQ = {
    q1: `jq '[.[]|select(.state=="open" and ((.title+" "+.body)|ascii_downcase|test("security")))]|length' issues.json`,
    q2: `jq '[.[]|select(.is_pr)]|length' issues.json`,
    q3: `jq '[.[]|select(.state=="closed").comments]|add' issues.json`,
    q4: `jq -r '[.[]|select(.is_pr).user]|group_by(.)|map({u:.[0],n:length})|sort_by(-.n)[0]|"\\(.u) \\(.n)"' issues.json`,
    q5: `jq -c '[.[]|select(.is_pr|not)|select(.state=="open" and .comments>5).number]|sort' issues.json`,
    q6: `jq '[.[].labels[]]|unique|length' issues.json`,
    q7: `jq '[.[]|select(.state=="open" and (.labels|index("bug")))]|length' issues.json`,
    q8: `jq '[.[].comments]|max' issues.json`,
    q9: `jq '([.[]|select(.is_pr).user]|unique) as $p|([.[]|select(.is_pr|not).user]|unique) as $i|[$p[]|select(. as $x|$i|index($x))]|length' issues.json`,
    q10: `jq '[.[]|select((.is_pr|not) and .state=="open").comments]|(add/length*100|round/100)' issues.json`,
};

const CORE = {
    q1: `c=0; for f in issues/*.json; do grep -q '"state": "open"' "$f" || continue; grep -E '^  "(title|body)":' "$f" | grep -qi security && c=$((c+1)); done; echo $c`,
    q2: `grep -l '"is_pr": true' issues/*.json | wc -l`,
    q3: `grep -l '"state": "closed"' issues/*.json | xargs grep -h '"comments":' | grep -o '[0-9]\\+' | awk '{s+=$1} END {print s}'`,
    q4: `grep -h '"user":' $(grep -l '"is_pr": true' issues/*.json) | sed 's/.*: "\\(.*\\)",/\\1/' | sort | uniq -c | sort -rn | head -1 | awk '{print $2, $1}'`,
    q5: `for f in issues/*.json; do grep -q '"is_pr": false' "$f" && grep -q '"state": "open"' "$f" && [ "$(grep -o '"comments": [0-9]*' "$f" | grep -o '[0-9]*')" -gt 5 ] && grep -o '"number": [0-9]*' "$f" | grep -o '[0-9]*'; done | sort -n | paste -sd,`,
    q6: `awk '/"labels": \\[\\]/{next} /"labels": \\[/{f=1;next} f&&/^  \\],?$/{f=0;next} f{gsub(/^[ \\t]*"|",?$/,"");print}' issues/*.json | sort -u | wc -l`,
    q7: `c=0; for f in issues/*.json; do grep -q '"state": "open"' "$f" || continue; awk '/"labels": \\[/{f=1;next} f&&/^  \\],?$/{f=0} f' "$f" | grep -q '"bug"' && c=$((c+1)); done; echo $c`,
    q8: `grep -h '"comments":' issues/*.json | grep -o '[0-9]\\+' | sort -n | tail -1`,
    q9: `p=$(mktemp); i=$(mktemp); grep -h '"user":' $(grep -l '"is_pr": true' issues/*.json) | sed 's/.*: "\\(.*\\)",/\\1/' | sort -u > "$p"; grep -h '"user":' $(grep -l '"is_pr": false' issues/*.json) | sed 's/.*: "\\(.*\\)",/\\1/' | sort -u > "$i"; comm -12 "$p" "$i" | wc -l; rm -f "$p" "$i"`,
    q10: `for f in $(grep -l '"is_pr": false' issues/*.json); do grep -q '"state": "open"' "$f" && grep -o '"comments": [0-9]*' "$f" | grep -o '[0-9]*'; done | awk '{s+=$1;n++} END {printf "%.2f\\n", s/n}'`,
};

const SQL = {
    q1: `SELECT count(*) FROM issues WHERE state='open' AND lower(title||' '||body) LIKE '%security%';`,
    q2: `SELECT count(*) FROM issues WHERE is_pr=1;`,
    q3: `SELECT sum(comments) FROM issues WHERE state='closed';`,
    q4: `SELECT user,count(*) n FROM issues WHERE is_pr=1 GROUP BY user ORDER BY n DESC LIMIT 1;`,
    q5: `SELECT group_concat(number) FROM (SELECT number FROM issues WHERE is_pr=0 AND state='open' AND comments>5 ORDER BY number);`,
    q6: `SELECT count(DISTINCT j.value) FROM issues, json_each(issues.labels) j;`,
    q7: `SELECT count(*) FROM issues WHERE state='open' AND EXISTS(SELECT 1 FROM json_each(issues.labels) j WHERE j.value='bug');`,
    q8: `SELECT max(comments) FROM issues;`,
    q9: `SELECT count(*) FROM (SELECT user FROM issues WHERE is_pr=1 INTERSECT SELECT user FROM issues WHERE is_pr=0);`,
    q10: `SELECT round(avg(comments),2) FROM issues WHERE is_pr=0 AND state='open';`,
};

const PS = {
    q1: `$d=Get-Content issues.json -Raw|ConvertFrom-Json; ($d|Where-Object{$_.state -eq "open" -and ($_.title+" "+$_.body).ToLower().Contains("security")}).Count`,
    q2: `$d=Get-Content issues.json -Raw|ConvertFrom-Json; ($d|Where-Object{$_.is_pr}).Count`,
    q3: `$d=Get-Content issues.json -Raw|ConvertFrom-Json; ($d|Where-Object{$_.state -eq "closed"}|Measure-Object comments -Sum).Sum`,
    q4: `$d=Get-Content issues.json -Raw|ConvertFrom-Json; $d|Where-Object{$_.is_pr}|Group-Object user|Sort-Object Count -Descending|Select-Object -First 1 Name,Count|ForEach-Object{"$($_.Name) $($_.Count)"}`,
    q5: `$d=Get-Content issues.json -Raw|ConvertFrom-Json; ($d|Where-Object{-not $_.is_pr -and $_.state -eq "open" -and $_.comments -gt 5}|Sort-Object number|ForEach-Object{$_.number}) -join ","`,
    q6: `$d=Get-Content issues.json -Raw|ConvertFrom-Json; ($d|ForEach-Object{$_.labels}|Sort-Object -Unique).Count`,
    q7: `$d=Get-Content issues.json -Raw|ConvertFrom-Json; ($d|Where-Object{$_.state -eq "open" -and $_.labels -contains "bug"}).Count`,
    q8: `$d=Get-Content issues.json -Raw|ConvertFrom-Json; ($d|Measure-Object comments -Maximum).Maximum`,
    q9: `$d=Get-Content issues.json -Raw|ConvertFrom-Json; $p=$d|Where-Object{$_.is_pr}|ForEach-Object{$_.user}|Sort-Object -Unique; $i=$d|Where-Object{-not $_.is_pr}|ForEach-Object{$_.user}|Sort-Object -Unique; ($p|Where-Object{$i -contains $_}).Count`,
    q10: `$d=Get-Content issues.json -Raw|ConvertFrom-Json; [math]::Round(($d|Where-Object{-not $_.is_pr -and $_.state -eq "open"}|Measure-Object comments -Average).Average,2)`,
};

const NU = {
    q1: `open issues.json | where {|r| $r.state == "open" and (($r.title + " " + $r.body) =~ "(?i)security")} | length`,
    q2: `open issues.json | where is_pr == true | length`,
    q3: `open issues.json | where state == "closed" | get comments | math sum`,
    q4: `open issues.json | where is_pr == true | group-by user | transpose k v | each {|x| {u: $x.k, n: ($x.v | length)}} | sort-by n | last | $"($in.u) ($in.n)"`,
    q5: `open issues.json | where {|r| $r.is_pr == false and $r.state == "open" and $r.comments > 5} | get number | sort | str join ","`,
    q6: `open issues.json | get labels | flatten | uniq | length`,
    q7: `open issues.json | where {|r| $r.state == "open" and ($r.labels | any {|l| $l == "bug"})} | length`,
    q8: `open issues.json | get comments | math max`,
    q9: `let p = (open issues.json | where is_pr == true | get user | uniq); let i = (open issues.json | where is_pr == false | get user | uniq); $p | where {|u| $u in $i} | length`,
    q10: `open issues.json | where {|r| $r.is_pr == false and $r.state == "open"} | get comments | math avg | math round -p 2`,
};

export const COMMANDS = {
    aethershell: AE, 'bash+jq': JQ, 'bash+coreutils': CORE,
    sqlite: SQL, pwsh: PS, nushell: NU,
};

export const QUESTIONS = {
    q1: 'How many open items mention "security" in title or body?',
    q2: 'How many of the 500 records are pull requests?',
    q3: 'Total comments across all closed items.',
    q4: 'Which author opened the most PRs, and how many?',
    q5: 'Numbers of open non-PR issues with more than 5 comments, sorted.',
    q6: 'How many distinct labels appear anywhere in the corpus?',
    q7: 'How many open items carry the "bug" label?',
    q8: 'Largest comment count on any item.',
    q9: 'How many authors have opened both a PR and an issue?',
    q10: 'Mean comment count on open non-PR issues, to 2dp.',
};

// Attempts to a correct command, recorded while this corpus was written.
// See README.md: the author was not blinded, so this measures the interaction
// of one author's fluency with each language, not the languages alone.
export const ATTEMPTS = {
    aethershell: { q1: 2, q2: 1, q3: 1, q4: 4, q5: 1, q6: 1, q7: 2, q8: 3, q9: 2, q10: 1 },
    'bash+jq': { q1: 1, q2: 1, q3: 1, q4: 1, q5: 1, q6: 1, q7: 1, q8: 1, q9: 1, q10: 1 },
    'bash+coreutils': { q1: 2, q2: 1, q3: 2, q4: 2, q5: 1, q6: 4, q7: 2, q8: 1, q9: 1, q10: 1 },
    sqlite: { q1: 1, q2: 1, q3: 1, q4: 1, q5: 1, q6: 1, q7: 1, q8: 1, q9: 1, q10: 1 },
    pwsh: { q1: 1, q2: 1, q3: 1, q4: 1, q5: 1, q6: 1, q7: 1, q8: 1, q9: 1, q10: 1 },
    nushell: { q1: 3, q2: 1, q3: 1, q4: 1, q5: 2, q6: 1, q7: 2, q8: 1, q9: 2, q10: 2 },
};
