# hmod port: skript
Round 1
- Fixed: src/lib.rs `has_permission` (script `permission:` nodes and `player has permission`): `PlayerRef::has_permission` replaced by a typed `permission_bool` query on a `PermissionNode` built from the node name; any error is a denial.
- Fixed: src/lib.rs `player_commands_sent` and `player_command`: `has_permission(SKRIPT_NODE)` replaced by typed `permission_bool` on `skript_node()`.
- Fixed: src/lib.rs `register_commands`: `Command::op(...).with_node(SKRIPT_NODE)` now passes `&skript_node()` (typed node).
- Fixed: src/lib.rs new `permission_gather_nodes` hook declares `skript.admin` with default level 2 (plugin.yml default op).
- Fixed: src/lib.rs `entities_in_world`: `EntityTypeKey` mapped to its id string with `kind.id(host)` before the mannequin comparison.
- Fixed: src/lib.rs `set_equipment` (living entity path): `ItemStack::set_display` is not on main; call removed, the name is computed but not set.
- Permission nodes:
  - `skript.admin` -> Level 2, declared in `permission_gather_nodes` (`skript_node()`), used by `/skript`, `/sk`, Tab filter and `player_command`.
  - Script-defined nodes (`permission:` lines and `has permission` with any text): not declared (they come from scripts at run time), so they are refused as unregistered.
- Gaps:
  - HMOD-GAP(script-permission-nodes): script-defined permission nodes cannot be declared at gathering, so every script permission check denies, operators included (the old level-2 fallback is gone for these). Only the permission-gated script branches are affected.
  - HMOD-GAP(item-display): `ItemStack::set_display` (custom name) does not exist on main. A living entity's equipment from `set_equipment` gets the bare item without its name.
- Unsure:
  - Undeclared script nodes are denied (spec: any error is a denial) rather than falling back to operator level 2 as before; this is a choice, not a verified equivalent.
