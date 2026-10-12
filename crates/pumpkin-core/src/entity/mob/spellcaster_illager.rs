//! Shared spellcasting state and goals for `SpellcasterIllager` subclasses.
//!
//! Vanilla keeps this in `net.minecraft.world.entity.monster.illager.SpellcasterIllager`. Both the
//! Evoker and the Illusioner hold a [`SpellcasterIllagerData`] and use
//! [`SpellcasterCastingSpellGoal`].

use std::sync::Weak;
use std::sync::atomic::{AtomicI32, AtomicU8, Ordering};

use pumpkin_data::sound::Sound;

use crate::entity::ai::goal::{Controls, Goal, to_goal_ticks};
use crate::entity::mob::Mob;
use crate::entity::mob::evoker::IllagerSpell;

/// Tick countdown and current spell, mirroring `SpellcasterIllager`'s fields.
pub struct SpellcasterIllagerData {
    spell_casting_tick_count: AtomicI32,
    current_spell: AtomicU8,
}

impl SpellcasterIllagerData {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            spell_casting_tick_count: AtomicI32::new(0),
            current_spell: AtomicU8::new(IllagerSpell::None as u8),
        }
    }

    #[must_use]
    pub fn is_casting_spell(&self) -> bool {
        self.get_spell_casting_time() > 0
    }

    #[must_use]
    pub fn get_spell_casting_time(&self) -> i32 {
        self.spell_casting_tick_count.load(Ordering::Relaxed)
    }

    pub fn set_spell_casting_time(&self, ticks: i32) {
        self.spell_casting_tick_count
            .store(ticks, Ordering::Relaxed);
    }

    #[must_use]
    pub fn current_spell(&self) -> IllagerSpell {
        IllagerSpell::from_u8(self.current_spell.load(Ordering::Relaxed))
    }

    pub fn set_current_spell(&self, spell: IllagerSpell) {
        self.current_spell.store(spell as u8, Ordering::Relaxed);
    }

    /// Vanilla `SpellcasterIllager.customServerAiStep` countdown.
    pub fn tick(&self) {
        let ticks = self.get_spell_casting_time();
        if ticks > 0 {
            self.set_spell_casting_time(ticks - 1);
        }
    }
}

impl Default for SpellcasterIllagerData {
    fn default() -> Self {
        Self::new()
    }
}

/// Implemented by mobs that extend `SpellcasterIllager`.
pub trait SpellcasterIllager: Mob {
    fn spellcaster_data(&self) -> &SpellcasterIllagerData;

    /// Vanilla `SpellcasterIllager.setIsCastingSpell`, which also syncs the spell id.
    fn set_is_casting_spell(&self, spell: IllagerSpell);
}

/// Vanilla `SpellcasterIllager.SpellcasterCastingSpellGoal`: while a spell is being cast the mob
/// stops moving and keeps looking at its target.
pub struct SpellcasterCastingSpellGoal {
    caster: Weak<dyn SpellcasterIllager>,
}

impl SpellcasterCastingSpellGoal {
    #[must_use]
    pub fn new(caster: Weak<dyn SpellcasterIllager>) -> Self {
        Self { caster }
    }

    fn is_casting_spell(&self) -> bool {
        self.caster
            .upgrade()
            .is_some_and(|caster| caster.spellcaster_data().get_spell_casting_time() > 0)
    }
}

impl Goal for SpellcasterCastingSpellGoal {
    fn can_start(&mut self, _mob: &dyn Mob) -> bool {
        self.is_casting_spell()
    }

    fn should_continue(&mut self, _mob: &dyn Mob) -> bool {
        self.is_casting_spell()
    }

    fn start(&mut self, mob: &dyn Mob) {
        mob.get_mob_entity()
            .navigator
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .stop();
    }

    fn stop(&mut self, _mob: &dyn Mob) {
        if let Some(caster) = self.caster.upgrade() {
            caster.set_is_casting_spell(IllagerSpell::None);
        }
    }

    fn tick(&mut self, mob: &dyn Mob) {
        if let Some(target) = mob.get_mob_entity().get_target() {
            mob.get_mob_entity()
                .look_control
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .look_at_entity(mob, &target);
        }
    }

    fn controls(&self) -> Controls {
        Controls::MOVE | Controls::LOOK
    }
}

/// Common state and logic of vanilla `SpellcasterIllager.SpellcasterUseSpellGoal`.
pub struct SpellcasterUseSpell {
    warmup_delay: i32,
    next_attack_tick: i32,
    cast_warmup_time: i32,
}

impl SpellcasterUseSpell {
    /// Vanilla `SpellcasterUseSpellGoal.getCastWarmupTime`.
    const CAST_WARMUP_TIME: i32 = 20;

    #[must_use]
    pub const fn new() -> Self {
        Self {
            warmup_delay: 0,
            next_attack_tick: 0,
            cast_warmup_time: Self::CAST_WARMUP_TIME,
        }
    }

    /// For subclasses that override `getCastWarmupTime`, like `Evoker.EvokerWololoSpellGoal`.
    #[must_use]
    pub const fn with_cast_warmup_time(cast_warmup_time: i32) -> Self {
        Self {
            warmup_delay: 0,
            next_attack_tick: 0,
            cast_warmup_time,
        }
    }

    /// Vanilla `SpellcasterUseSpellGoal.canUse` timing, without the target check. Used by goals
    /// that forbid a target, like `Evoker.EvokerWololoSpellGoal`.
    #[must_use]
    pub fn can_start_ignoring_target(&self, mob: &dyn Mob, is_casting_spell: bool) -> bool {
        !is_casting_spell && mob.get_entity().age.load(Ordering::Relaxed) >= self.next_attack_tick
    }

    /// Vanilla `SpellcasterUseSpellGoal.canUse`.
    #[must_use]
    pub fn can_start(&self, mob: &dyn Mob, is_casting_spell: bool) -> bool {
        let Some(target) = mob.get_mob_entity().get_target() else {
            return false;
        };
        target.get_entity().is_alive() && self.can_start_ignoring_target(mob, is_casting_spell)
    }

    /// Vanilla `SpellcasterUseSpellGoal.canContinueToUse`.
    #[must_use]
    pub fn can_continue(&self, mob: &dyn Mob) -> bool {
        self.warmup_delay > 0
            && mob
                .get_mob_entity()
                .get_target()
                .is_some_and(|target| target.get_entity().is_alive())
    }

    /// `attackWarmupDelay > 0`, i.e. the spell is still warming up.
    #[must_use]
    pub const fn is_warming_up(&self) -> bool {
        self.warmup_delay > 0
    }

    /// Vanilla `SpellcasterUseSpellGoal.start`, minus the concrete spell parameters.
    pub fn start(
        &mut self,
        mob: &dyn Mob,
        caster: &dyn SpellcasterIllager,
        casting_time: i32,
        casting_interval: i32,
        spell: IllagerSpell,
        prepare_sound: Option<Sound>,
    ) {
        self.warmup_delay = to_goal_ticks(self.cast_warmup_time);
        caster
            .spellcaster_data()
            .set_spell_casting_time(casting_time);
        self.next_attack_tick = mob.get_entity().age.load(Ordering::Relaxed) + casting_interval;
        if let Some(sound) = prepare_sound {
            mob.get_entity().play_sound(sound);
        }
        caster.set_is_casting_spell(spell);
    }

    /// Vanilla `SpellcasterUseSpellGoal.tick`. Returns `true` on the tick the spell is cast.
    pub const fn tick(&mut self) -> bool {
        self.warmup_delay -= 1;
        self.warmup_delay == 0
    }
}

impl Default for SpellcasterUseSpell {
    fn default() -> Self {
        Self::new()
    }
}
