import argparse
import logging
from gcalendar_helper import get_calendar_service, fetch_events
from zoneinfo import ZoneInfo
from calendar_to_service import month_range, load_config
from calendar_report import generate_report
from typing import Dict


def main() -> None:
    logging.basicConfig(level=logging.WARNING, format="%(levelname)s: %(message)s")

    parser = argparse.ArgumentParser(description="Generate service report from Google Calendar")
    parser.add_argument(
        "--service-prefix", dest="service_prefix", default=None, help="Prefix to filter event summaries"
    )
    parser.add_argument("--month", type=int, required=True, help="Month number (1-12)")
    parser.add_argument("--year", type=int, required=True, help="Year (e.g., 2026)")
    parser.add_argument("--calendar-config", default="calendar-config.yaml", help="Path to calendar config file")
    parser.add_argument("--rates-config", default="rates.yaml", help="Path to rates config file")
    parser.add_argument("--config", default=None, help="Deprecated: use --calendar-config")

    args = parser.parse_args()

    calendar_config_path = args.calendar_config
    if args.config:
        calendar_config_path = args.config

    calendar_config = load_config(calendar_config_path)
    rates_config = load_config(args.rates_config)

    google_cfg = calendar_config.get("google", {})
    calendar_id = google_cfg.get("calendar_id", "primary")
    timezone_name = google_cfg.get("timezone", "UTC")
    service_costs = rates_config.get("service_costs", [])

    cost_lookup: Dict[str, Dict[str, float]] = {}
    if isinstance(service_costs, list):
        for entry in service_costs:
            name = entry.get("name") if isinstance(entry, dict) else None
            per_person = entry.get("per_person_hourly") if isinstance(entry, dict) else None
            if not name or not isinstance(per_person, dict):
                continue
            normalized = {}
            for person, rate in per_person.items():
                try:
                    normalized[str(person)] = float(rate)
                except (TypeError, ValueError):
                    logging.warning("Invalid hourly rate for %s / %s", name, person)
            cost_lookup[str(name)] = normalized

    try:
        tz = ZoneInfo(timezone_name)
    except Exception as exc:
        raise ValueError(f"Invalid timezone in config: {timezone_name}") from exc

    start, end = month_range(args.year, args.month, tz)

    service = get_calendar_service(calendar_config)
    events = fetch_events(service, calendar_id, start, end)

    report = generate_report(events, args.month, args.year, tz, args.service_prefix, cost_lookup)
    print(report)


if __name__ == "__main__":
    main()
