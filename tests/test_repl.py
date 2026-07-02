import io
import subprocess
import sys
import types

import pytest

from measurekit.repl import main


def test_one_shot_expression(capsys):
    assert main(["500 N / 2 m^2 => Pa"]) == 0
    assert "250" in capsys.readouterr().out


def test_one_shot_multiple_statements(capsys):
    assert main(["x = 2 m; x => cm"]) == 0
    assert "200" in capsys.readouterr().out


def test_one_shot_error_exits_nonzero(capsys):
    assert main(["5 furlonx"]) == 1
    assert "furlonx" in capsys.readouterr().err


def test_piped_stdin(monkeypatch, capsys):
    monkeypatch.setattr(
        "sys.stdin", io.StringIO("force = 500 N\nforce => kN\n")
    )
    assert main([]) == 0
    assert "0.5" in capsys.readouterr().out


def test_interactive_session(monkeypatch, capsys):
    lines = iter(
        [
            "force = 500 N",
            "force => kN",
            "(oops",  # parse error must not kill the session
            "exit",
        ]
    )
    monkeypatch.setattr(
        "sys.stdin", types.SimpleNamespace(isatty=lambda: True)
    )
    monkeypatch.setattr("builtins.input", lambda prompt="": next(lines))
    assert main([]) == 0
    captured = capsys.readouterr()
    assert "0.5" in captured.out
    assert "error:" in captured.err


def test_interactive_eof_exits(monkeypatch, capsys):
    def raise_eof(prompt=""):
        raise EOFError

    monkeypatch.setattr(
        "sys.stdin", types.SimpleNamespace(isatty=lambda: True)
    )
    monkeypatch.setattr("builtins.input", raise_eof)
    assert main([]) == 0


def test_cli_repl_subcommand(monkeypatch, capsys):
    from measurekit import cli

    monkeypatch.setattr("sys.argv", ["measurekit", "repl", "1 km => m"])
    with pytest.raises(SystemExit) as excinfo:
        cli.main()
    assert excinfo.value.code == 0
    assert "1000" in capsys.readouterr().out


def test_python_dash_m_entry_point():
    result = subprocess.run(
        [sys.executable, "-m", "measurekit", "1 km => m"],
        capture_output=True,
        text=True,
        timeout=120,
        check=False,
    )
    assert result.returncode == 0
    assert "1000" in result.stdout
