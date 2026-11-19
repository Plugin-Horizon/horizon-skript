//! Tests for the fixes of the parity audit (see the commit messages): permission nodes of scripts,
//! command headers and arguments, partial loading, `variables:` and `aliases:`.

use mod_skript::horizon_skript_core::manager::permission_nodes_in;

#[test]
fn permission_nodes_of_a_script_are_found() {
    let src = "command /a:\n\
               \tpermission: coresmp.a.admin\n\
               \tpermission message: <red>No</red>\n\
               \ttrigger:\n\
               \t\tif player has permission \"x.y\":\n\
               \t\t\tstop\n\
               \t\tif player has permission \"%{_n}%\":\n\
               \t\t\tstop\n\
               # permission: no.comment\n";
    assert_eq!(
        permission_nodes_in(src),
        vec!["coresmp.a.admin".to_owned(), "x.y".to_owned()]
    );
}
