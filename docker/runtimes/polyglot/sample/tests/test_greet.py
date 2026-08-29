from sample import greet


def test_greet() -> None:
    assert greet("rig") == "hello rig"
