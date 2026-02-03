# Calendar Services Report

CLI that reads Google Calendar events for a given month/year, parses `Service: Person` from event summaries, and outputs a Markdown report per person with day/hour details. Events that do not match the pattern emit warnings.

## Setup

1. Create a Google Cloud project and enable **Google Calendar API**.
2. Create OAuth 2.0 **Desktop** credentials and download the client secrets JSON.
3. Copy the example config and fill in paths.

```
cp calendar-config.yaml.example calendar-config.yaml
cp rates.yaml.example rates.yaml
```

4. Install dependencies:

```
pip install -r requirements.txt
```

## Usage

```
python calendar_report.py --service-prefix "Explicação" --month 2 --year 2026
```

### Flags
- `--service-prefix`: Prefix used to filter events (summary must start with it).
- `--month`: Month number (1–12).
- `--year`: Full year (e.g., 2026).
- `--calendar-config`: Path to calendar config (default: `calendar-config.yaml`).
- `--rates-config`: Path to rates config (default: `rates.yaml`).

## Output

The tool prints Markdown tables grouped by person with columns: **Day**, **Start**, **End**, **Hours**, **Cost**, plus a **Total** row summing costs.

## HTTP Server

Serve a computed Markdown report over HTTP (Flask):

```
python report_server.py --calendar-config calendar-config.yaml --rates-config rates.yaml --host 127.0.0.1 --port 8000
```

Request the report with path parameters (HTML response):

```
http://127.0.0.1:8000/report/2026/1?service_prefix=Explica%C3%A7%C3%A3o
```

### Rates endpoint

Show current hourly rates for all students:

```
http://127.0.0.1:8000/rates
```

### Update rate (POST)

Update a student's hourly rate (service optional). If service is omitted and the student does not exist, the student is added to the first service or to a Default service if none exist.

```
POST http://127.0.0.1:8000/rates
Content-Type: application/json

{
	"student": "Joana",
	"rate": 15
}

### Delete rate (DELETE)

Remove a student's rate:

```
DELETE http://127.0.0.1:8000/rates
Content-Type: application/json

{
	"student": "Joana"
}
```
```

## Service Costs

Add service costs in the config file. The `name` must match the service prefix (the part before `:` in the event summary). Each service can define per-person hourly rates.

## Notes
- Multi-day events are split by day.
- All-day events are treated as 24h blocks per day.
- Warnings are shown for events that do not match `Service: Person`.
