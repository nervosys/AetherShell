# Web Scraping

Examples of web scraping and data extraction with AetherShell.

## Basic Page Fetching

```aethershell
# Fetch a page
let page = http_get "https://example.com"
echo page.status     # 200
echo page.ok         # true

# Extract text content
let text = web_html_to_text page.body
echo text
```

## Scraping with Selectors

```aethershell
# Scrape article titles
let titles = web_scrape "https://news.ycombinator.com" "a.storylink"
titles | take 10 | each(fn(t) => echo t.text)
```

## API-Based Scraping

```aethershell
# GitHub repository stats
let repos = ["rust-lang/rust", "tokio-rs/tokio", "serde-rs/serde"]

repos | map(fn(repo) => {
  let data = web_json_get "https://api.github.com/repos/${repo}"
  {
    name: data.full_name,
    stars: data.stargazers_count,
    forks: data.forks_count,
    language: data.language
  }
}) | sort_by "stars" "desc"
```

## Content Extraction

### HTML to Markdown

```aethershell
let html = (http_get "https://blog.example.com/post/1").body
let md = web_html_to_markdown html
file_write "post.md" md
```

### Extract Emails

```aethershell
let page = (http_get "https://example.com/contact").body
let emails = web_extract_emails page
echo emails   # ["info@example.com", "support@example.com"]
```

### Extract Links

```aethershell
let links = web_scrape "https://example.com" "a[href]"
links | map(fn(a) => a.href) | where(fn(h) => starts_with h "https://") | unique
```

## Download Pipeline

```aethershell
# Download multiple files
let urls = [
  "https://data.example.com/dataset1.csv",
  "https://data.example.com/dataset2.csv",
  "https://data.example.com/dataset3.csv"
]

mkdir "downloads"
urls | each(fn(url) => {
  let filename = last(split(url, "/"))
  echo "Downloading ${filename}..."
  web_download url "downloads/${filename}"
})
```

## URL Health Checking

```aethershell
let urls = cat "urls.txt" | split "\n" | where(fn(u) => len(u) > 0)

let results = urls | map(fn(url) => {
  let check = web_check_url url
  { url: url, status: check.status, ok: check.reachable }
})

# Report broken links
let broken = results | where(fn(r) => !r.ok)
echo "${len broken} broken links found:"
broken | each(fn(r) => echo "  ✗ ${r.url} (${r.status})")
```

## Paginated API

```aethershell
# Fetch all pages from a paginated API
let all_items = []
let page = 1
let has_more = true

# Note: AetherShell supports while loops via recursion
let fetch_page = fn(page, acc) => {
  let data = web_json_get "https://api.example.com/items?page=${page}&limit=100"
  let items = concat acc data.items
  if len(data.items) == 100 {
    fetch_page (page + 1) items
  } else {
    items
  }
}

let all_items = fetch_page 1 []
echo "Total items: ${len all_items}"
```

## RSS Feed Parsing

```aethershell
let feed = (http_get "https://blog.example.com/feed.xml").body
let items = web_xpath feed "//item"

items | take 5 | map(fn(item) => {
  {
    title: web_xpath item "title/text()",
    link: web_xpath item "link/text()",
    date: web_xpath item "pubDate/text()"
  }
})
```
