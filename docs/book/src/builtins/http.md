# HTTP & Networking

IronShell provides comprehensive networking builtins for HTTP requests, web automation, and low-level network operations. All return structured data for pipeline processing.

## HTTP Requests

### `http_get`
Make an HTTP GET request. Returns a Record with status, headers, body, and ok fields.

```ironshell
let resp = http_get "https://api.github.com/repos/rust-lang/rust"
echo resp.status     # 200
echo resp.ok         # true

# Parse JSON response
let repo = json_parse resp.body
echo repo.stargazers_count
```

### `web_get` / `curl`
Extended GET request with options support.

```ironshell
web_get "https://api.example.com/data" {
  headers: { "Authorization": "Bearer ${token}" },
  timeout: 5000
}
```

### `web_post`
Make a POST request with a body.

```ironshell
let resp = web_post "https://api.example.com/items" {
  headers: { "Content-Type": "application/json" },
  body: json_stringify { name: "widget", price: 9.99 }
}
```

### `web_fetch` / `fetch`
General-purpose HTTP client supporting all methods.

```ironshell
let resp = web_fetch "https://api.example.com/items/1" {
  method: "PUT",
  headers: { "Content-Type": "application/json" },
  body: json_stringify { name: "updated widget" }
}
```

## JSON APIs

### `web_json_get`
GET request that automatically parses the JSON response body.

```ironshell
let users = web_json_get "https://api.example.com/users"
users | where(fn(u) => u.active) | map(fn(u) => u.name)
```

### `web_json_post`
POST request with automatic JSON serialization/deserialization.

```ironshell
let result = web_json_post "https://api.example.com/login" {
  username: "admin",
  password: "secret"
}
echo result.token
```

## Web Scraping

### `web_scrape` / `scrape`
Scrape content from a web page with CSS selectors.

```ironshell
let titles = web_scrape "https://news.ycombinator.com" "a.storylink"
titles | take 10 | each(fn(t) => echo t.text)
```

### `web_html_to_text`
Extract plain text from HTML content.

```ironshell
let html = (http_get "https://example.com").body
let text = web_html_to_text html
```

### `web_html_to_markdown`
Convert HTML content to Markdown.

```ironshell
let md = web_html_to_markdown (http_get "https://example.com").body
file_write "page.md" md
```

### `web_extract_emails`
Extract email addresses from text or HTML.

```ironshell
let emails = web_extract_emails (cat "contacts.html")
echo emails   # ["user@example.com", ...]
```

### `web_extract_phones`
Extract phone numbers from text.

```ironshell
let phones = web_extract_phones page_content
```

## URL Operations

### `web_parse_url`
Parse a URL into its components.

```ironshell
web_parse_url "https://example.com:8080/path?key=val#section"
# { scheme: "https", host: "example.com", port: 8080, path: "/path", query: "key=val", fragment: "section" }
```

### `web_encode_url` / `web_decode_url`
URL-encode or decode a string.

```ironshell
web_encode_url "hello world & more"    # "hello%20world%20%26%20more"
web_decode_url "hello%20world"          # "hello world"
```

### `web_parse_query` / `web_build_query`
Parse query strings to Records, or build query strings from Records.

```ironshell
web_parse_query "name=Ada&lang=Rust"
# { name: "Ada", lang: "Rust" }

web_build_query { page: 2, limit: 50 }
# "page=2&limit=50"
```

### `web_check_url`
Check if a URL is reachable (returns status info).

```ironshell
let check = web_check_url "https://example.com"
echo check.reachable    # true
echo check.status       # 200
```

## Downloads

### `web_download`
Download a file from a URL.

```ironshell
web_download "https://example.com/data.csv" "downloads/data.csv"
```

### `web_open_url`
Open a URL in the system default browser.

```ironshell
web_open_url "https://docs.ironshell.dev"
```

## Advanced Web

### `web_rest_api`
High-level REST API client with authentication and pagination support.

```ironshell
let client = web_rest_api "https://api.example.com" {
  auth: { type: "bearer", token: env.API_TOKEN },
  base_headers: { "Accept": "application/json" }
}
```

### `web_websocket`
Connect to a WebSocket endpoint.

```ironshell
let ws = web_websocket "wss://stream.example.com/events"
```

### `web_graphql`
Execute a GraphQL query.

```ironshell
let result = web_graphql "https://api.example.com/graphql" {
  query: "{ users { id name email } }",
  variables: { limit: 10 }
}
```

### `web_headers` / `web_cookies`
Extract headers or cookies from a response.

```ironshell
let h = web_headers (http_get "https://example.com")
echo h["Content-Type"]
```

### `web_robots_txt` / `web_sitemap`
Fetch and parse robots.txt or sitemap.xml.

```ironshell
let robots = web_robots_txt "https://example.com"
let sitemap = web_sitemap "https://example.com"
```

### `web_json_path` / `web_xpath`
Query JSON with JSONPath or HTML with XPath expressions.

```ironshell
let data = web_json_get "https://api.example.com/data"
web_json_path data "$.users[*].name"

let html = (http_get "https://example.com").body
web_xpath html "//h1/text()"
```

## Network Operations

### `net_interfaces` / `ifconfig`
List network interfaces with their addresses and status.

```ironshell
net_interfaces
# [{ name: "eth0", ip: "192.168.1.100", mac: "aa:bb:cc:dd:ee:ff", up: true }, ...]
```

### `net_ip`
Get the machine's IP address.

```ironshell
echo (net_ip)    # 192.168.1.100
```

### `net_dns_lookup`
Resolve a hostname to IP addresses.

```ironshell
net_dns_lookup "example.com"
# ["93.184.216.34"]
```

### `net_ping` / `ping`
Ping a host and return latency information.

```ironshell
net_ping "google.com"
# { host: "google.com", latency_ms: 12.5, reachable: true }
```

### `net_ports`
List open ports on the local machine.

```ironshell
net_ports | where(fn(p) => p.state == "LISTEN")
```

### `net_connections`
List active network connections.

```ironshell
net_connections
  | where(fn(c) => c.remote_port == 443)
  | map(fn(c) => c.remote_addr)
  | unique
```

### `net_whois`
WHOIS lookup for a domain.

```ironshell
net_whois "example.com"
```

### `net_traceroute`
Trace the route to a host.

```ironshell
net_traceroute "example.com"
# [{ hop: 1, ip: "192.168.1.1", latency_ms: 1.2 }, ...]
```

### `net_stats` / `net_bandwidth`
Network statistics and bandwidth usage.

```ironshell
net_stats
# [{ section: "Tcp", counter: "active connection openings", value: 1234 }, ...]
```

## Pipeline Examples

```ironshell
# API data analysis
web_json_get "https://api.github.com/repos/rust-lang/rust/contributors?per_page=100"
  | map(fn(c) => { login: c.login, commits: c.contributions })
  | sort_by "commits" "desc"
  | take 10

# Health check multiple endpoints
let endpoints = ["https://api1.example.com/health", "https://api2.example.com/health"]
endpoints
  | map(fn(url) => {
      let check = web_check_url url
      { url: url, status: check.status, ok: check.reachable }
  })
  | where(fn(e) => !e.ok)

# Download and process CSV
web_download "https://data.example.com/export.csv" "/tmp/data.csv"
let rows = cat "/tmp/data.csv" | split "\n" | map(fn(line) => split line ",")
echo "Loaded ${len rows} rows"
```
