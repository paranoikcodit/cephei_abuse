import base64
import sys

sys.path.append(".venv\\lib\\python3.11\\site-packages")

from pathlib import Path
from opentele.td import TDesktop
import struct
import socket
from sqlite3 import DatabaseError, Row, connect

from pyrogram.session.internals.data_center import DataCenter

STRING_SIZE = 351
STRING_SIZE_64 = 356


class StructFormats:
    Telethon = {"default": ">B{}sH256s"}
    Pyrogram = {"default": ">BI?256sQ?", "old": ">B?256sI?", "old_64": ">B?256sQ?"}


class TelethonSession:
    TABLES = {
        "sessions": {"dc_id", "server_address", "port", "auth_key", "takeout_id"},
        "entities": {"id", "hash", "username", "phone", "name", "date"},
        "sent_files": {"md5_digest", "file_size", "type", "id", "hash"},
        "update_state": {"id", "pts", "qts", "date", "seq"},
        "version": {"version"},
    }

    def __init__(
        self,
        *,
        dc_id: int,
        auth_key: bytes,
        server_address: None | str = None,
        port: None | int = None,
        takeout_id: None | int = None,
    ):
        self.dc_id = dc_id
        self.auth_key = auth_key
        self.server_address = server_address
        self.port = port
        self.takeout_id = takeout_id

    @classmethod
    def validate(cls, path: str) -> bool:
        try:
            with connect(path) as db:
                db.row_factory = Row
                sql = "SELECT name FROM sqlite_master WHERE type='table'"
                cursor = db.execute(sql)

                tables = {row["name"] for row in cursor.fetchall()}

                if tables != set(cls.TABLES.keys()):
                    return False

                for table, session_columns in cls.TABLES.items():
                    sql = f'pragma table_info("{table}")'
                    cur = db.execute(sql)
                    columns = {row["name"] for row in cur.fetchall()}
                    if session_columns != columns:
                        return False

        except DatabaseError:
            return False

        return True


class PyrogramSession:
    TABLES = {
        "sessions": {"dc_id", "test_mode", "auth_key", "date", "user_id", "is_bot"},
        "peers": {
            "id",
            "access_hash",
            "type",
            "username",
            "phone_number",
            "last_update_on",
        },
        "version": {"number"},
    }

    def __init__(
        self,
        *,
        dc_id: int,
        auth_key: bytes,
        user_id: None | int = None,
        is_bot: bool = False,
        test_mode: bool = False,
        api_id: None | int = None,
        date: int | None = None,
    ):
        self.dc_id = dc_id
        self.auth_key = auth_key
        self.user_id = user_id
        self.is_bot = is_bot
        self.test_mode = test_mode
        self.api_id = api_id

    @classmethod
    def validate(cls, path: str) -> bool:
        try:
            with connect(path) as db:
                db.row_factory = Row
                sql = "SELECT name FROM sqlite_master WHERE type='table'"
                cursor = db.execute(sql)
                tables = {row["name"] for row in cursor.fetchall()}

                if tables != set(cls.TABLES.keys()):
                    return False

                for table, session_columns in cls.TABLES.items():
                    sql = f'pragma table_info("{table}")'
                    cur = db.execute(sql)

                    columns = {row["name"] for row in cur.fetchall()}

                    if "api_id" in columns:
                        columns.remove("api_id")
                    if session_columns != columns:
                        return False

        except DatabaseError:
            return False

        return True


def exists(target: str):
    try:
        return Path(target).exists()
    except:
        return False


def ip2int(addr):
    return struct.unpack("!I", socket.inet_aton(addr))[0]


def convert_telethon_to_session(string_or_file: str):
    if exists(string_or_file):
        if TelethonSession.validate(string_or_file):
            with connect(string_or_file) as db:
                db.row_factory = Row
                cursor = db.execute("select * from sessions")

                session = cursor.fetchone()
                session = TelethonSession(**session)

                return serialize_telegram_data_to_session(
                    session.dc_id,
                    session.server_address,
                    session.port,
                    session.auth_key,
                )
        else:
            raise Exception("File is not valid")
    else:
        string = string_or_file[1:]
        ip_len = 4 if len(string) == 352 else 16
        dc_id, ip, port, auth_key = struct.unpack(
            StructFormats.Telethon["default"].format(ip_len),
            base64.urlsafe_b64decode(string),
        )

        ip = int.from_bytes(ip)

        return serialize_telegram_data_to_session(dc_id, ip, port, auth_key)


def convert_pyrogram_to_session(string_or_file: str):
    if exists(string_or_file):
        if PyrogramSession.validate(string_or_file):
            with connect(string_or_file) as db:
                db.row_factory = Row
                cursor = db.execute("select * from sessions")

                session = cursor.fetchone()
                session = PyrogramSession(**session)

                ip, port = DataCenter(session.dc_id, False, False, False)

                return serialize_telegram_data_to_session(
                    session.dc_id,
                    ip,
                    port,
                    session.auth_key,
                )
    else:
        if len(string_or_file) in [STRING_SIZE, STRING_SIZE_64]:
            string_format = StructFormats.Pyrogram["old_64"]

            if len(string_or_file) == STRING_SIZE:
                string_format = StructFormats.Pyrogram["old"]

            api_id = None
            dc_id, test_mode, auth_key, user_id, is_bot = struct.unpack(
                string_format,
                base64.urlsafe_b64decode(
                    string_or_file + "=" * (-len(string_or_file) % 4)
                ),
            )
        else:
            dc_id, api_id, test_mode, auth_key, user_id, is_bot = struct.unpack(
                StructFormats.Pyrogram["default"],
                base64.urlsafe_b64decode(
                    string_or_file + "=" * (-len(string_or_file) % 4)
                ),
            )

        ip, port = DataCenter(dc_id, False, False, False)

        return serialize_telegram_data_to_session(dc_id, ip, port, auth_key)


def serialize_telegram_data_to_session(
    dc_id: int, ip: str | int, port: int, auth_key: bytes
) -> bytes:
    session = (2805905614).to_bytes(4, "little")
    session += (0).to_bytes(4, "little")
    session += (481674261).to_bytes(4, "little")
    session += (1).to_bytes(4, "little", signed=True)
    session += (1970083510).to_bytes(4, "little")

    session += (0 | 1 | 0 | 4).to_bytes(4, "little")

    session += dc_id.to_bytes(4, "little")

    ip_: int = None

    if type(ip) is str:
        ip_ = ip2int(ip)
    else:
        ip_ = ip

    session += ip_.to_bytes(4, "little")
    session += port.to_bytes(4, "little")

    len_ = 0
    if len(auth_key) <= 253:
        session += bytes([len(auth_key)])
        len_ = len(auth_key) + 1
    else:
        session += bytes(
            [
                254,
                len(auth_key) & 0xFF,
                (len(auth_key) >> 8) & 0xFF,
                (len(auth_key) >> 16) & 0xFF,
            ]
        )

        len_ = len(auth_key)

    padding = (4 - (len_ % 4)) % 4

    session += auth_key
    session += bytes([0 for _ in range(0, padding)])

    return session


def convert_tdata_to_session(source: str) -> bytes:
    tdata = TDesktop(source)

    account = tdata.accounts[0]
    endpoints = account._local.config.endpoints(account.MainDcId)
    endpoint = endpoints[0][0][0]

    return serialize_telegram_data_to_session(
        endpoint.id, endpoint.ip, endpoint.port, account.authKey.key
    )


def detect_session_format(session_or_file: str):
    if exists(session_or_file):
        try:
            TDesktop(session_or_file)
            return "TData"
        except:
            if PyrogramSession.validate(session_or_file):
                return "Pyrogram"
            elif TelethonSession.validate(session_or_file):
                return "Telethon"
            return "Unknown"

    try:
        convert_telethon_to_session(session_or_file)
        return "Telethon"
    except:
        pass

    try:
        convert_pyrogram_to_session(session_or_file)
        return "Pyrogram"
    except:
        pass

    return "Unknown"
