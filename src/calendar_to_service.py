import datetime as dt
import logging
from typing import Dict, Iterable, Optional, Tuple
import os

import yaml
from dateutil import parser as date_parser


def load_config(path: str) -> Dict:
    if not os.path.isfile(path):
        raise FileNotFoundError(f"Config file not found: {path}")
    with open(path, "r", encoding="utf-8") as handle:
        return yaml.safe_load(handle) or {}


def month_range(year: int, month: int, tz: dt.tzinfo) -> Tuple[dt.datetime, dt.datetime]:
    start = dt.datetime(year, month, 1, 0, 0, 0, tzinfo=tz)
    if month == 12:
        end = dt.datetime(year + 1, 1, 1, 0, 0, 0, tzinfo=tz)
    else:
        end = dt.datetime(year, month + 1, 1, 0, 0, 0, tzinfo=tz)
    return start, end


def parse_event_datetimes(event: Dict, tz: dt.tzinfo) -> Tuple[dt.datetime, dt.datetime, bool]:
    start = event.get("start", {})
    end = event.get("end", {})

    if "dateTime" in start:
        start_dt = date_parser.isoparse(start["dateTime"])
        end_dt = date_parser.isoparse(end["dateTime"])
        all_day = False
    else:
        start_dt = date_parser.isoparse(start.get("date"))
        end_dt = date_parser.isoparse(end.get("date"))
        start_dt = dt.datetime.combine(start_dt.date(), dt.time.min)
        end_dt = dt.datetime.combine(end_dt.date(), dt.time.min)
        all_day = True

    if start_dt.tzinfo is None:
        start_dt = start_dt.replace(tzinfo=tz)
    if end_dt.tzinfo is None:
        end_dt = end_dt.replace(tzinfo=tz)

    start_dt = start_dt.astimezone(tz)
    end_dt = end_dt.astimezone(tz)
    return start_dt, end_dt, all_day


def split_event_by_day(
    start_dt: dt.datetime,
    end_dt: dt.datetime,
    tz: dt.tzinfo,
) -> Iterable[Tuple[dt.date, dt.datetime, dt.datetime, float]]:
    if end_dt <= start_dt:
        return []

    results = []
    current_day = start_dt.date()
    last_day = end_dt.date()

    while current_day <= last_day:
        day_start = dt.datetime.combine(current_day, dt.time.min, tzinfo=tz)
        day_end = day_start + dt.timedelta(days=1)
        segment_start = max(start_dt, day_start)
        segment_end = min(end_dt, day_end)

        if segment_end > segment_start:
            hours = (segment_end - segment_start).total_seconds() / 3600
            results.append((current_day, segment_start, segment_end, hours))

        current_day += dt.timedelta(days=1)

    return results


def parse_summary(summary: str, prefix: Optional[str]) -> Optional[Tuple[str, str]]:
    if prefix:
        if not summary.startswith(prefix):
            return None

    if " - " not in summary:
        return None

    service, person = summary.split(" - ", 1)
    service = service.strip()
    person = person.strip()

    if not service or not person:
        return None

    return service, person


def calculate_month_totals(
    events: Iterable[Dict],
    month: int,
    year: int,
    tz: dt.tzinfo,
    prefix: Optional[str],
    cost_lookup: Dict[str, Dict[str, float]],
) -> Tuple[float, float]:
    total_hours = 0.0
    total_cost = 0.0
    missing_service_costs = set()
    missing_person_costs = set()

    def resolve_hourly_rate(service_name: str, person: str) -> Optional[float]:
        if service_name not in cost_lookup:
            if service_name not in missing_service_costs:
                logging.warning("Missing cost config for service: %s", service_name)
                missing_service_costs.add(service_name)
            return None

        person_rates = cost_lookup[service_name]
        if person not in person_rates:
            key = (service_name, person)
            if key not in missing_person_costs:
                logging.warning("Missing cost config for service/person: %s / %s", service_name, person)
                missing_person_costs.add(key)
            return None

        return person_rates[person]

    for event in events:
        summary = event.get("summary", "")
        parsed = parse_summary(summary, prefix)
        if parsed is None:
            if prefix and summary.startswith(prefix):
                logging.warning("Invalid summary format: %s", summary)
            elif not prefix:
                logging.warning("Invalid summary format: %s", summary)
            continue

        service_name, person = parsed

        try:
            start_dt, end_dt, _ = parse_event_datetimes(event, tz)
        except Exception as exc:
            logging.warning("Failed to parse event datetime (%s): %s", summary, exc)
            continue

        hourly_rate = resolve_hourly_rate(service_name, person)

        for day, _, _, hours in split_event_by_day(start_dt, end_dt, tz):
            if day.year != year or day.month != month:
                continue
            total_hours += hours
            if hourly_rate is not None:
                total_cost += hours * hourly_rate

    return total_hours, total_cost
