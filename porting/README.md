# Porting vanilla behavior into Pumpkin

How to check a Pumpkin feature against vanilla and close the gaps. Written so the next agent can
skip the setup work and go straight to the comparison. Per-feature logs live next to this file
(`blaze.md`, ...).

## Ground rules

- **Port behavior, not code.** Read vanilla to learn the rules (timings, thresholds, goal
  priorities, ordering), then write it against Pumpkin's own APIs. No line-by-line transpiles, no
  copied Java, no decompiled source committed anywhere in this repo. Logs describe behavior and
  name vanilla classes; they don't reproduce them.
- Decompiled output stays in a scratch dir outside the repo (`/root/mc` below).
- Prefer Pumpkin's existing goals/utilities over new ones. If a vanilla piece has no Pumpkin
  equivalent, use the closest one and leave a `TODO:` naming the vanilla class.

## 1. Find the target version

```sh
grep -rn CURRENT_MC_VERSION crates/pumpkin-world/src/lib.rs   # e.g. "26.3"
```

## 2. Get the vanilla server jar

Mojang's version manifest is reachable from the sandbox:

```sh
mkdir -p /root/mc && cd /root/mc
curl -sS https://piston-meta.mojang.com/mc/game/version_manifest_v2.json \
  | python3 -c "import json,sys;d=json.load(sys.stdin);[print(v['id'],v['url']) for v in d['versions'][:10]]"
curl -sS <version-json-url> -o ver.json
python3 -c "import json;print(json.load(open('ver.json'))['downloads']['server']['url'])"
curl -sS -o server.jar <server-url>
```

**26.x jars are not obfuscated** — the version JSON has no `server_mappings`, and class/method
names are already Mojang's. No remapping step needed. (For pre-26 versions you'd need the
`server_mappings` ProGuard file and a remapper.)

The downloaded jar is a bundler; the real classes are inside:

```sh
unzip -o -q server.jar 'META-INF/versions/*/server-*.jar'
mv META-INF/versions/*/server-*.jar ./server-inner.jar
```

## 3. Decompile only what you need

Vineflower from Maven Central (also reachable):

```sh
curl -sS -o vineflower.jar https://repo1.maven.org/maven2/org/vineflower/vineflower/1.11.1/vineflower-1.11.1.jar
unzip -l server-inner.jar | grep -i <feature>            # find the classes
unzip -o -q server-inner.jar 'net/minecraft/world/entity/monster/Blaze*' -d cls
java -jar vineflower.jar -log=WARN cls src               # a few seconds for a handful of classes
```

Decompiling the whole jar works too but takes minutes; targeted extraction is enough for a
single mob. Include inner classes (`Foo$Bar.class`) — goals are usually inner classes.

Useful vanilla locations (26.x package layout):

| What | Where |
|---|---|
| Mobs | `net/minecraft/world/entity/monster/`, `.../animal/`, `.../ambient/` |
| Shared goals | `net/minecraft/world/entity/ai/goal/` and `.../goal/target/` |
| Projectiles | `net/minecraft/world/entity/projectile/` (fireballs under `hurtingprojectile/`) |
| Water/rain damage | `LivingEntity.aiStep` (`isSensitiveToWater` + `isInWaterOrRain`) |
| Data (loot, tags) | `data/minecraft/...` inside the inner jar — Pumpkin generates most of this already |

## 4. Where things live in Pumpkin

| Vanilla | Pumpkin |
|---|---|
| Mob class | `crates/pumpkin-core/src/entity/mob/<name>.rs` |
| `registerGoals` | goal/target selectors set up in the mob's `new()` |
| Goal classes | `crates/pumpkin-core/src/entity/ai/goal/*.rs` (`HurtByTargetGoal` → `revenge.rs`, `NearestAttackableTargetGoal` → `active_target.rs`, `RandomStrollGoal` → `wander_around.rs`, `FloatGoal` → `swim.rs`) |
| `aiStep` (always runs) | `Mob::mob_tick` |
| `customServerAiStep` (skipped with NoAI) | `Mob::custom_server_ai_step` — runs after navigation, before move/look controls, same slot as vanilla |
| `setPathfindingMalus` | `mob.entity.navigator.lock().set_pathfinding_malus(PathType::..)`. Vanilla `FIRE_IN_NEIGHBOR` = `DangerFire`, `FIRE` = `DamageFire` |
| Attributes (`FOLLOW_RANGE`, ...) | generated from data; read with `living_entity.get_attribute_value(&Attributes::X)` |
| `random.triangle(a, b)` | `rand::rng().triangle(a, b)` via `ai::util::RandomExt` |
| Synced data flags | `entity.set_synced_data(pumpkin_data::tracked_data::<mob>::..)` |
| `needsSync = true` after velocity change | `entity.velocity_dirty.store(true, ..)` |
| `isSilent()` | `entity.is_silent()` |
| Projectile damage with owner | `damage_with_context(target, amount, type, Some(hit_pos), Some(projectile), owner)` — `source` is the direct entity, `cause` is the attacker |
| `gameRules MOB_GRIEFING` | `world.level_info.load().game_rules.mob_griefing` |
| Placing fire like `BaseFireBlock.getState` | `FireBlockBase::get_fire_type` + `FireBlock.get_state_for_position` |

Sounds, XP reward, loot and fire immunity mostly come from generated entity-type data, so check
`pumpkin-data` before porting those by hand.

## 5. Workflow that worked

1. Read the Pumpkin mob file and its goals end to end.
2. Read the vanilla mob class and list every behavior: constructor (malus, xp), goals with
   priorities, target goals, attributes, `aiStep`, `customServerAiStep`, overrides
   (`isSensitiveToWater`, `isOnFire`, sounds), and the projectile it fires.
3. Diff the two lists. For each gap, find the Pumpkin hook from the table above.
4. Implement, `cargo check -p pumpkin`, `cargo clippy -p pumpkin-core`, run related tests.
5. Write a `<feature>.md` log: what matched, what was fixed, what's still open and why.

## 6. Build notes

- First `cargo check -p pumpkin` from cold is slow (several minutes); start it early in the
  background and read code while it runs.
- `cargo test -p pumpkin-core <filter>` for targeted tests.
