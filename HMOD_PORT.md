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
  - (closed in round 2) HMOD-GAP(script-permission-nodes): the nodes the script files name are now declared at gathering.
  - HMOD-GAP(item-display): `ItemStack::set_display` (custom name) does not exist on main. A living entity's equipment from `set_equipment` gets the bare item without its name.
- Unsure:
  - Undeclared script nodes are denied (spec: any error is a denial) rather than falling back to operator level 2 as before; this is a choice, not a verified equivalent.

Round 2 (parity audit)
- Fixed: script `permission:` and `has permission "..."` nodes are scanned from the scripts folder and declared (level 2) in `permission_gather_nodes`; an undeclared (dynamic) node answers for operators only, as Bukkit does; `skript.effectcommands`, `skript.reloadnotify` and `skript.see_runtime_errors` are declared too.
- Fixed: script commands run from the console and parse their header (`<type>`, `[<type=default>]`, `<name: type>`, words), with `executable by`, `cooldown`, `aliases`; a wrong line prints the usage; `arg-N`, `arg-<type>`, `<type>-arg`, `last arg` and named arguments work.
- Fixed: a trigger or function with an error is left out and the rest of the file loads; `variables:`, `aliases:`, `using`, `local function` and `with priority` are accepted.
- Fixed: a reload, enable or disable that changes the script commands calls `refresh_commands`; script aliases are registered.
- Fixed: `on chat` runs at once; `cancel event` and `set message to` act on the message.
- Fixed: `/skript` help, `reload all|scripts|config|aliases|last|<script or folder>`, `enable|disable all|<folder>`, Tab completion, reload notices for `skript.reloadnotify`.
- Gaps: HMOD-GAP(damage-cancel): `living_incoming_damage` gets a level host, which cannot run the scripts, so `cancel event` in `on damage` has no effect. First join stays approximated (no join-history call). `every day at <time>` and the missing syntax classes remain.
- Unsure: a node that is not declared is answered with permission level >= 2 (operator), a choice that follows Bukkit's default, not a verified equivalent. Command usage lines print without a prefix, as Skript's own usage did.
