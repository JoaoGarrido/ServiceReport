import collections
import datetime as dt
import logging
import html
from typing import Dict, List, Optional, Tuple
from calendar_to_service import parse_event_datetimes, split_event_by_day, parse_summary


def generate_report(
    events: List[Dict],
    month: int,
    year: int,
    tz: dt.tzinfo,
    prefix: Optional[str],
    cost_lookup: Dict[str, Dict[str, float]],
) -> str:
    rows_by_person = collections.defaultdict(list)
    missing_service_costs = set()
    missing_person_costs = set()

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

        for day, segment_start, segment_end, hours in split_event_by_day(start_dt, end_dt, tz):
            if day.year != year or day.month != month:
                continue
            rows_by_person[person].append(
                {
                    "day": day,
                    "start": segment_start,
                    "end": segment_end,
                    "hours": hours,
                    "service": service_name,
                    "person": person,
                }
            )

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

    def format_time(start_time: dt.datetime, end_time: dt.datetime) -> Tuple[str, str]:
        start_str = start_time.strftime("%H:%M")
        if end_time.time() == dt.time.min and end_time.date() != start_time.date():
            end_str = "24:00"
        else:
            end_str = end_time.strftime("%H:%M")
        return start_str, end_str

    lines = []
    for person in sorted(rows_by_person.keys()):
        lines.append(f"## {person}")
        lines.append("")
        lines.append("| Day | Start | End | Hours | Cost |")
        lines.append("| --- | ---: | ---: | ---: | ---: |")

        rows = sorted(
            rows_by_person[person],
            key=lambda item: (item["day"], item["start"], item["end"]),
        )

        total_cost = 0.0
        for item in rows:
            start_str, end_str = format_time(item["start"], item["end"])
            hourly_rate = resolve_hourly_rate(item["service"], person)
            if hourly_rate is None:
                cost_display = "-"
            else:
                cost_value = item["hours"] * hourly_rate
                total_cost += cost_value
                cost_display = f"{cost_value:.2f}"
            lines.append(
                f"| {item['day'].isoformat()} | {start_str} | {end_str} | {item['hours']:.2f} | {cost_display} |"
            )

        if rows:
            lines.append(f"| **Total** |  |  |  | {total_cost:.2f} |")

        lines.append("")

    if not lines:
        lines.append("No matching events found.")

    return "\n".join(lines)


def generate_report_html(
    events: List[Dict],
    month: int,
    year: int,
    tz: dt.tzinfo,
    prefix: Optional[str],
    cost_lookup: Dict[str, Dict[str, float]],
) -> str:
    rows_by_person = collections.defaultdict(list)
    missing_service_costs = set()
    missing_person_costs = set()

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

        for day, segment_start, segment_end, hours in split_event_by_day(start_dt, end_dt, tz):
            if day.year != year or day.month != month:
                continue
            rows_by_person[person].append(
                {
                    "day": day,
                    "start": segment_start,
                    "end": segment_end,
                    "hours": hours,
                    "service": service_name,
                }
            )

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

    def format_time(start_time: dt.datetime, end_time: dt.datetime) -> Tuple[str, str]:
        start_str = start_time.strftime("%H:%M")
        if end_time.time() == dt.time.min and end_time.date() != start_time.date():
            end_str = "24:00"
        else:
            end_str = end_time.strftime("%H:%M")
        return start_str, end_str

    if not rows_by_person:
        return "<p>No matching events found.</p>"

    total_hours = 0.0
    total_earned = 0.0
    for person_name, items in rows_by_person.items():
        for item in items:
            total_hours += item["hours"]
            rate = resolve_hourly_rate(item["service"], person_name)
            if rate is not None:
                total_earned += item["hours"] * rate

    parts = [
        "<html>",
        "<head>",
        "<meta charset=\"utf-8\" />",
        "<style>",
        "table { border-collapse: collapse; width: 100%; }",
        "th, td { border: 1px solid #ccc; padding: 6px 8px; text-align: right; }",
        "th:first-child, td:first-child { text-align: left; }",
        "th:nth-child(2), th:nth-child(3), td:nth-child(2), td:nth-child(3) { text-align: center; }",
        "h2 { margin-top: 24px; }",
        "</style>",
        "</head>",
        "<body>",
        "<p><strong>Congratualations!</strong></p>",
        f"<p>Hours worked {total_hours:.2f}</p>",
        f"<p>Earned {total_earned:.2f} this month</p>",
    ]

    for person in sorted(rows_by_person.keys()):
        parts.append(f"<h2>{html.escape(person)}</h2>")
        parts.append("<table>")
        parts.append("<thead><tr><th>Day</th><th>Start</th><th>End</th><th>Hours</th><th>Cost</th></tr></thead>")
        parts.append("<tbody>")

        rows = sorted(
            rows_by_person[person],
            key=lambda item: (item["day"], item["start"], item["end"]),
        )

        total_cost = 0.0
        for item in rows:
            start_str, end_str = format_time(item["start"], item["end"])
            hourly_rate = resolve_hourly_rate(item["service"], person)
            if hourly_rate is None:
                cost_display = "-"
            else:
                cost_value = item["hours"] * hourly_rate
                total_cost += cost_value
                cost_display = f"{cost_value:.2f}"

            parts.append(
                "<tr>"
                f"<td>{html.escape(item['day'].isoformat())}</td>"
                f"<td>{html.escape(start_str)}</td>"
                f"<td>{html.escape(end_str)}</td>"
                f"<td>{item['hours']:.2f}</td>"
                f"<td>{html.escape(cost_display)}</td>"
                "</tr>"
            )

        if rows:
            parts.append(
                "<tr>"
                "<td><strong>Total</strong></td>"
                "<td></td><td></td><td></td>"
                f"<td><strong>{total_cost:.2f}</strong></td>"
                "</tr>"
            )

        parts.append("</tbody></table>")

    parts.extend(["</body>", "</html>"])
    return "\n".join(parts)

