from app.domain.fans import engine


def evaluate_fans(**context) -> dict:
    return engine.evaluate_fan_context(context)
