from google.auth.transport.requests import Request
from google.oauth2.credentials import Credentials
from google_auth_oauthlib.flow import InstalledAppFlow
from googleapiclient.discovery import build
from typing import Dict, List
import os
import datetime as dt


SCOPES = ["https://www.googleapis.com/auth/calendar.readonly"]


def get_calendar_service(config: Dict):
    google_cfg = config.get("google", {})
    client_secret_file = google_cfg.get("client_secret_file")
    token_file = google_cfg.get("token_file", "token.json")

    if not client_secret_file:
        raise ValueError("Missing google.client_secret_file in config")

    creds = None
    if os.path.exists(token_file):
        creds = Credentials.from_authorized_user_file(token_file, SCOPES)
    if not creds or not creds.valid:
        if creds and creds.expired and creds.refresh_token:
            creds.refresh(Request())
        else:
            flow = InstalledAppFlow.from_client_secrets_file(client_secret_file, SCOPES)
            creds = flow.run_local_server(port=0)
        with open(token_file, "w", encoding="utf-8") as token:
            token.write(creds.to_json())

    return build("calendar", "v3", credentials=creds)


def fetch_events(service, calendar_id: str, time_min: dt.datetime, time_max: dt.datetime) -> List[Dict]:
    events = []
    page_token = None
    while True:
        response = (
            service.events()
            .list(
                calendarId=calendar_id,
                timeMin=time_min.isoformat(),
                timeMax=time_max.isoformat(),
                singleEvents=True,
                orderBy="startTime",
                pageToken=page_token,
            )
            .execute()
        )
        events.extend(response.get("items", []))
        page_token = response.get("nextPageToken")
        if not page_token:
            break
    return events
