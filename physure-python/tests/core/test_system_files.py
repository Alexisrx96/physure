"""A system file is read fail-closed.

A bare name (``imperial.conf``) is a file shipped in
``physure.infrastructure.config.systems``: it is read from the installed
package or the call fails naming it. It is never looked up in the working
directory (a same-named file there must not be loaded instead), and a missing
one never quietly builds a base-catalog system. A name with a directory part
is the path of a file of your own, read or refused the same way. The
working-directory ``physure.conf`` override is a different, documented
mechanism and still applies.
"""

import pytest

from physure.application.startup import create_system
from physure.domain.exceptions import UnknownUnitError

DECOY = "[System]\nname = Decoy\n[Units]\nzzdecoy = 3, L\n"


def test_a_missing_packaged_system_file_raises_naming_it():
    with pytest.raises(FileNotFoundError, match=r"absent_system\.conf"):
        create_system("absent_system.conf")


def test_a_same_named_file_in_the_working_directory_is_not_used(
    tmp_path, monkeypatch
):
    (tmp_path / "decoy_system.conf").write_text(DECOY, encoding="utf-8")
    monkeypatch.chdir(tmp_path)
    with pytest.raises(FileNotFoundError, match=r"decoy_system\.conf"):
        create_system("decoy_system.conf")


def test_a_packaged_file_wins_over_a_same_named_file_in_the_working_directory(
    tmp_path, monkeypatch
):
    (tmp_path / "imperial.conf").write_text(DECOY, encoding="utf-8")
    monkeypatch.chdir(tmp_path)
    system = create_system("imperial.conf")
    assert system.name == "Imperial"
    with pytest.raises(UnknownUnitError):
        system.get_unit("zzdecoy")


def test_a_path_names_a_file_of_your_own(tmp_path):
    own = tmp_path / "own.conf"
    own.write_text(DECOY, encoding="utf-8")
    system = create_system(str(own))
    assert system.name == "Decoy"
    assert system.get_unit("zzdecoy") is not None


def test_a_missing_path_raises_naming_it(tmp_path):
    missing = tmp_path / "missing.conf"
    with pytest.raises(FileNotFoundError, match=r"missing\.conf"):
        create_system(str(missing))


def test_the_working_directory_physure_conf_still_overrides(
    tmp_path, monkeypatch
):
    (tmp_path / "physure.conf").write_text(
        "[Units]\nzzoverride = 2, L\n", encoding="utf-8"
    )
    monkeypatch.chdir(tmp_path)
    system = create_system("international.conf")
    assert system.get_unit("zzoverride") is not None


def test_the_shipped_systems_are_packaged():
    assert create_system("imperial.conf").name == "Imperial"
    assert create_system("international.conf").name == "SI"


def test_pydantic_system_names_resolve_to_shipped_files():
    pydantic = pytest.importorskip("pydantic")
    from physure import Quantity

    class Model(pydantic.BaseModel, arbitrary_types_allowed=True):
        q: Quantity

    # "SI" (the default) is international.conf, whose [System] name is SI
    assert Model(q={"magnitude": 2, "unit": "m"}).q.system.name == "SI"
    imperial = {"magnitude": 2, "unit": "ft", "system": "Imperial"}
    assert Model(q=imperial).q.system.name == "Imperial"
    with pytest.raises(pydantic.ValidationError, match=r"nosuch\.conf"):
        Model(q={"magnitude": 2, "unit": "m", "system": "nosuch"})
