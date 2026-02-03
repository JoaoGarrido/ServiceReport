import argparse
import datetime as dt
import logging
import os

from zoneinfo import ZoneInfo

from flask import Flask, jsonify, request, Response, render_template

import calendar_report
import calendar_to_service
import gcalendar_helper
import yaml


def create_app(calendar_config_path: str, rates_config_path: str) -> Flask:
    app = Flask(__name__, template_folder="templates")
    app.config["CALENDAR_CONFIG_PATH"] = calendar_config_path
    app.config["RATES_CONFIG_PATH"] = rates_config_path

    @app.get("/")
    def home():
        return render_template("home.html")

    @app.get("/rates")
    def get_rates():
        rates_config = request.args.get("rates_config", app.config["RATES_CONFIG_PATH"])
        response_format = request.args.get("format", "markdown")
        try:
            rates_list = build_rates_list(rates_config)
            if response_format == "json":
                return jsonify({"rates": rates_list})
            content = build_rates_markdown_from_list(rates_list)
        except Exception as exc:
            logging.exception("Failed to build rates")
            return jsonify({"error": str(exc)}), 500
        return Response(content, mimetype="text/markdown")

    @app.get("/rates-ui")
    def get_rates_ui():
        calendar_config = request.args.get("calendar_config", app.config["CALENDAR_CONFIG_PATH"])
        rates_config = request.args.get("rates_config", app.config["RATES_CONFIG_PATH"])
        try:
            hours_worked, earned = build_current_month_totals(calendar_config, rates_config)
        except Exception as exc:
            logging.exception("Failed to build rates summary")
            return jsonify({"error": str(exc)}), 500

        return render_template(
            "rates.html",
            hours_worked=f"{hours_worked:.2f}",
            earned=f"{earned:.2f}",
        )

    @app.post("/rates")
    def post_rates():
        rates_config = request.args.get("rates_config", app.config["RATES_CONFIG_PATH"])
        payload = request.get_json(silent=True)
        if payload is None:
            return jsonify({"error": "Invalid JSON"}), 400

        student = payload.get("student")
        rate = payload.get("rate")
        if not student or rate is None:
            return jsonify({"error": "Fields required: student, rate"}), 400

        try:
            rate_value = float(rate)
        except (TypeError, ValueError):
            return jsonify({"error": "Invalid rate"}), 400

        service_name = payload.get("service")
        try:
            update_rate(rates_config, service_name, str(student), rate_value)
        except Exception as exc:
            logging.exception("Failed to update rate")
            return jsonify({"error": str(exc)}), 500

        return jsonify({"status": "ok"})

    @app.delete("/rates")
    def delete_rates():
        rates_config = request.args.get("rates_config", app.config["RATES_CONFIG_PATH"])
        payload = request.get_json(silent=True)
        if payload is None:
            return jsonify({"error": "Invalid JSON"}), 400

        student = payload.get("student")
        if not student:
            return jsonify({"error": "Field required: student"}), 400

        try:
            removed = delete_rate(rates_config, str(student))
        except Exception as exc:
            logging.exception("Failed to delete rate")
            return jsonify({"error": str(exc)}), 500

        if not removed:
            return jsonify({"error": "Student not found"}), 404

        return jsonify({"status": "ok"})

    @app.get("/report/<int:year>/<int:month>")
    def get_report(year: int, month: int):
        calendar_config = request.args.get("calendar_config", app.config["CALENDAR_CONFIG_PATH"])
        rates_config = request.args.get("rates_config", app.config["RATES_CONFIG_PATH"])
        service_prefix = request.args.get("service_prefix")

        try:
            report = build_report(calendar_config, rates_config, service_prefix, month, year)
        except Exception as exc:
            logging.exception("Failed to build report")
            return jsonify({"error": str(exc)}), 500

        return Response(report, mimetype="text/html")

    return app


def build_cost_lookup(config: dict) -> dict:
    service_costs = config.get("service_costs", [])
    cost_lookup = {}
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
    return cost_lookup


def build_rates_list(rates_config_path: str) -> list[dict]:
        config = calendar_to_service.load_config(rates_config_path)
        cost_lookup = build_cost_lookup(config)

        rows = []
        for service_name in sorted(cost_lookup.keys()):
                for person in sorted(cost_lookup[service_name].keys()):
                        rows.append({"student": person, "rate": cost_lookup[service_name][person]})

        return rows


def build_rates_markdown_from_list(rows: list[dict]) -> str:
        lines = ["# Rates", "", "| Student | Hourly Rate |", "| --- | ---: |"]

        for row in rows:
                lines.append(f"| {row['student']} | {row['rate']:.2f} |")

        if not rows:
                lines.append("No rates configured.")

        return "\n".join(lines)


def build_current_month_totals(calendar_config_path: str, rates_config_path: str) -> tuple[float, float]:
    calendar_config = calendar_to_service.load_config(calendar_config_path)
    google_cfg = calendar_config.get("google", {})
    calendar_id = google_cfg.get("calendar_id", "primary")
    timezone_name = google_cfg.get("timezone", "UTC")

    try:
        tz = ZoneInfo(timezone_name)
    except Exception as exc:
        raise ValueError(f"Invalid timezone in config: {timezone_name}") from exc

    now = dt.datetime.now(tz)
    start, end = calendar_to_service.month_range(now.year, now.month, tz)
    service = gcalendar_helper.get_calendar_service(calendar_config)
    events = gcalendar_helper.fetch_events(service, calendar_id, start, end)
    rates_config = calendar_to_service.load_config(rates_config_path)
    cost_lookup = build_cost_lookup(rates_config)
    return calendar_to_service.calculate_month_totals(events, now.month, now.year, tz, None, cost_lookup)




def build_report(calendar_config_path: str, rates_config_path: str, service_prefix: str, month: int, year: int) -> str:
    calendar_config = calendar_to_service.load_config(calendar_config_path)
    google_cfg = calendar_config.get("google", {})
    calendar_id = google_cfg.get("calendar_id", "primary")
    timezone_name = google_cfg.get("timezone", "UTC")

    try:
        tz = ZoneInfo(timezone_name)
    except Exception as exc:
        raise ValueError(f"Invalid timezone in config: {timezone_name}") from exc

    start, end = calendar_to_service.month_range(year, month, tz)
    service = gcalendar_helper.get_calendar_service(calendar_config)
    events = gcalendar_helper.fetch_events(service, calendar_id, start, end)
    rates_config = calendar_to_service.load_config(rates_config_path)
    cost_lookup = build_cost_lookup(rates_config)
    return calendar_report.generate_report_html(events, month, year, tz, service_prefix, cost_lookup)


def update_rate(rates_config_path: str, service_name: str, student: str, rate: float) -> None:
    config = calendar_to_service.load_config(rates_config_path)
    service_costs = config.get("service_costs")
    if not isinstance(service_costs, list):
        service_costs = []
        config["service_costs"] = service_costs

    if service_name:
        target_entry = None
        for entry in service_costs:
            if isinstance(entry, dict) and entry.get("name") == service_name:
                target_entry = entry
                break

        if target_entry is None:
            target_entry = {"name": service_name, "per_person_hourly": {}}
            service_costs.append(target_entry)

        per_person = target_entry.get("per_person_hourly")
        if not isinstance(per_person, dict):
            per_person = {}
            target_entry["per_person_hourly"] = per_person

        per_person[student] = rate
    else:
        updated = False
        for entry in service_costs:
            if not isinstance(entry, dict):
                continue
            per_person = entry.get("per_person_hourly")
            if not isinstance(per_person, dict):
                continue
            if student in per_person:
                per_person[student] = rate
                updated = True

        if not updated:
            target_entry = None
            for entry in service_costs:
                if isinstance(entry, dict) and isinstance(entry.get("per_person_hourly"), dict):
                    target_entry = entry
                    break

            if target_entry is None:
                target_entry = {"name": "Default", "per_person_hourly": {}}
                service_costs.append(target_entry)

            target_entry["per_person_hourly"][student] = rate

    write_config_with_backup(rates_config_path, config)


def delete_rate(rates_config_path: str, student: str) -> bool:
    config = calendar_to_service.load_config(rates_config_path)
    service_costs = config.get("service_costs")
    if not isinstance(service_costs, list):
        return False

    removed = False
    for entry in service_costs:
        if not isinstance(entry, dict):
            continue
        per_person = entry.get("per_person_hourly")
        if not isinstance(per_person, dict):
            continue
        if student in per_person:
            per_person.pop(student, None)
            removed = True

    if removed:
        write_config_with_backup(rates_config_path, config)

    return removed


def write_config_with_backup(config_path: str, config: dict) -> None:
    timestamp = dt.datetime.now().strftime("%Y%m%d_%H%M%S")
    backup_path = f"{config_path}.{timestamp}.bak"
    if os.path.exists(config_path):
        with open(config_path, "rb") as source, open(backup_path, "wb") as target:
            target.write(source.read())
    with open(config_path, "w", encoding="utf-8") as handle:
        yaml.safe_dump(config, handle, sort_keys=False, allow_unicode=True)


def main() -> None:
    parser = argparse.ArgumentParser(description="Serve a markdown report over HTTP")
    parser.add_argument("--config", default="config.yaml", help="Path to calendar config file")
    parser.add_argument("--rates", default="rates.yaml", help="Path to rates file")
    parser.add_argument("--host", default="127.0.0.1", help="Host to bind (default: 127.0.0.1)")
    parser.add_argument("--port", type=int, default=8000, help="Port to bind (default: 8000)")

    args = parser.parse_args()
    logging.basicConfig(level=logging.INFO, format="%(levelname)s: %(message)s")
    app = create_app(args.config, args.rates)
    app.run(host=args.host, port=args.port)


if __name__ == "__main__":
    main()
