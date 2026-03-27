from __future__ import annotations

from pydantic import Field
from pydantic_settings import BaseSettings, SettingsConfigDict


class Settings(BaseSettings):
    database_url: str = Field(default="sqlite+pysqlite:///:memory:")
    test_mode: bool = Field(default=False)

    model_config = SettingsConfigDict(env_prefix="MAHJONG_", extra="ignore")


def get_settings() -> Settings:
    return Settings()
