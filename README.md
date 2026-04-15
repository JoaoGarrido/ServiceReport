# Google Calendar Service Monthly Report

Application to allow easy monthly reports for independent service workers: e.g tutoring classes and other regular jobs by paid by hour.

The workflow uses Google Calendar as the backend/database of your services to a certain client, as so, you only need to keep track of your Google Calendar and your monthly report will be calculated afterwards.
It's recommended to have a separated calendar for your services.

Will parse the events from your calendar assuming a `Service: Client` pattern from event summaries and generates a monthly report.
- Assumes that rates are per client
- Events that do not match the pattern emit warnings.
- Events to clients without a rate emit warnings.
- Multi-day events are split by day.
- All-day events are treated as 24h blocks per day.

## Setup

1. Create a Google Cloud project and enable **Google Calendar API**.
2. Create OAuth 2.0 **Desktop** credentials and download the client secrets JSON.
3. Copy the example config and fill in paths.

```
cp calendar-config.yaml.example calendar-config.yaml
cp rates.yaml.example rates.yaml
```

4. Build the project:

```
cargo build --release
```

## Usage

```
cargo run --release -- report --service-prefix "Explicação" --month 2 --year 2026
```

Or run the binary directly:

```
./target/release/service-report --service-prefix "Explicação" --month 2 --year 2026
```

### Flags
- `--service-prefix`: Prefix used to filter events (summary must start with it).
- `--month`: Month number (1–12).
- `--year`: Full year (e.g., 2026).
- `--calendar-config`: Path to calendar config (default: `calendar-config.yaml`).
- `--rates-config`: Path to rates config (default: `rates.yaml`).

## Output

The tool prints Markdown tables grouped by client with columns: **Day**, **Start**, **End**, **Hours**, **Cost**, plus a **Total** row summing costs.

## HTTP Server

An HTTP server provides a simple interface to the CLI.

```
cargo run --release -- serve --host 127.0.0.1 --port 8000
```

The homepage can be accessed afterwards in the browser through:
127.0.0.1:8000

### Report/<year>/<month> endpoint

Request the report with path parameters (HTML response):

```
http://127.0.0.1:8000/report/2026/1
```

### Rates endpoint

Show current hourly rates for all students:

```
http://127.0.0.1:8000/rates
```

Will output in markdown when called directly and in JSON when called from the rates-ui.

### Update rate (POST)


**NOTE:**
*A change of rate of certain client will lead to a different monthly report of previous months.`*

Update a student's hourly rate (service optional). If service is omitted and the student does not exist, the student is added to the first service or to a Default service if none exist.

```
POST http://127.0.0.1:8000/rates
Content-Type: application/json

{
	"student": "Joana",
	"rate": 15
}
```

### Delete rate (DELETE)

Remove a student's rate:

```
DELETE http://127.0.0.1:8000/rates
Content-Type: application/json

{
	"student": "Joana"
}
```

## Service Costs

Add service costs in the config file. The `name` must match the service prefix (the part before `:` in the event summary). Each service can define per-client hourly rates.~
