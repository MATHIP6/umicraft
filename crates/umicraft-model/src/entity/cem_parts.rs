// SPDX-License-Identifier: MIT OR Apache-2.0

//! The part names of custom entity models (CEM) mapped to the fields of the
//! hard-coded mob models.
//!
//! Source: the part-name table of the public OptiFine CEM documentation
//! (`cem_model.txt`, section "entity part names"). The names are those of
//! the vanilla 1.8 models (MCP field names) with the same meaning, so a
//! pack model that replaces `leg1` replaces the part this crate calls
//! `leg1`. A name the table does not list has no counterpart here and the
//! entry that uses it is skipped.
//!
//! Covered: creeper, zombie, zombie pigman, skeleton, spider, cow, pig,
//! sheep, chicken, wolf, enderman, villager and iron golem. Not covered:
//! slime, snow golem, horse and the other mobs, whose models have no
//! documented 1.8 part table here yet.

use super::{
    ChickenModel, CreeperModel, EndermanModel, HumanoidModel, IronGolemModel, ModelPart,
    QuadrupedModel, SheepModel, SpiderModel, VillagerModel, WolfModel,
};

/// A mob model whose parts a custom model can replace or extend.
pub trait CemMob {
    /// The part the CEM name `name` stands for.
    fn cem_part(&self, name: &str) -> Option<&ModelPart>;

    /// Mutable access to the part the CEM name `name` stands for.
    fn cem_part_mut(&mut self, name: &str) -> Option<&mut ModelPart>;
}

/// Implements [`CemMob`] from a list of `"cem name" => field`.
macro_rules! cem_mob {
    ($model:ty { $($name:literal => $field:ident),+ $(,)? }) => {
        impl CemMob for $model {
            fn cem_part(&self, name: &str) -> Option<&ModelPart> {
                match name {
                    $($name => Some(&self.$field),)+
                    _ => None,
                }
            }

            fn cem_part_mut(&mut self, name: &str) -> Option<&mut ModelPart> {
                match name {
                    $($name => Some(&mut self.$field),)+
                    _ => None,
                }
            }
        }
    };
}

cem_mob!(CreeperModel {
    "head" => head, "body" => body,
    "leg1" => leg1, "leg2" => leg2, "leg3" => leg3, "leg4" => leg4,
});

cem_mob!(HumanoidModel {
    "head" => head, "body" => body,
    "left_arm" => left_arm, "right_arm" => right_arm,
    "left_leg" => left_leg, "right_leg" => right_leg,
});

cem_mob!(SpiderModel {
    "head" => head, "neck" => neck, "body" => body,
    "leg1" => leg1, "leg2" => leg2, "leg3" => leg3, "leg4" => leg4,
    "leg5" => leg5, "leg6" => leg6, "leg7" => leg7, "leg8" => leg8,
});

cem_mob!(QuadrupedModel {
    "head" => head, "body" => body,
    "leg1" => leg1, "leg2" => leg2, "leg3" => leg3, "leg4" => leg4,
});

cem_mob!(SheepModel {
    "head" => head, "body" => body,
    "leg1" => leg1, "leg2" => leg2, "leg3" => leg3, "leg4" => leg4,
});

cem_mob!(ChickenModel {
    "head" => head, "body" => body, "bill" => bill, "chin" => chin,
    "right_leg" => right_leg, "left_leg" => left_leg,
    "right_wing" => right_wing, "left_wing" => left_wing,
});

cem_mob!(WolfModel {
    "head" => head, "body" => body, "mane" => mane, "tail" => tail,
    "leg1" => leg1, "leg2" => leg2, "leg3" => leg3, "leg4" => leg4,
});

cem_mob!(EndermanModel {
    "head" => head, "headwear" => headwear, "body" => body,
    "left_arm" => left_arm, "right_arm" => right_arm,
    "left_leg" => left_leg, "right_leg" => right_leg,
});

cem_mob!(VillagerModel {
    "head" => head, "nose" => nose, "body" => body, "arms" => arms,
    "right_leg" => right_leg, "left_leg" => left_leg,
});

cem_mob!(IronGolemModel {
    "head" => head, "body" => body,
    "left_arm" => left_arm, "right_arm" => right_arm,
    "left_leg" => left_leg, "right_leg" => right_leg,
});

#[cfg(test)]
mod tests {
    use super::*;

    /// Every name resolves, to distinct parts, and an unknown name does not.
    fn check<M: CemMob>(model: &mut M, names: &[&str]) {
        for name in names {
            assert!(model.cem_part(name).is_some(), "{name} resolves");
            assert!(
                model.cem_part_mut(name).is_some(),
                "{name} resolves mutably"
            );
        }
        assert!(model.cem_part("no_such_part").is_none());
        // Distinct fields: marking one part must not mark another.
        for (i, name) in names.iter().enumerate() {
            model.cem_part_mut(name).unwrap().rotation[2] = 1.0 + i as f32;
        }
        for (i, name) in names.iter().enumerate() {
            assert_eq!(
                model.cem_part(name).unwrap().rotation[2],
                1.0 + i as f32,
                "{name}"
            );
        }
    }

    const BIPED: [&str; 6] = [
        "head",
        "body",
        "left_arm",
        "right_arm",
        "left_leg",
        "right_leg",
    ];
    const FOUR_LEGS: [&str; 6] = ["head", "body", "leg1", "leg2", "leg3", "leg4"];

    #[test]
    fn creeper_names() {
        check(&mut CreeperModel::new(), &FOUR_LEGS);
    }

    #[test]
    fn zombie_and_skeleton_names() {
        check(&mut HumanoidModel::zombie(), &BIPED);
        check(&mut HumanoidModel::skeleton(), &BIPED);
    }

    #[test]
    fn spider_names() {
        check(
            &mut SpiderModel::new(),
            &[
                "head", "neck", "body", "leg1", "leg2", "leg3", "leg4", "leg5", "leg6", "leg7",
                "leg8",
            ],
        );
    }

    #[test]
    fn cow_pig_and_sheep_names() {
        check(&mut QuadrupedModel::cow(), &FOUR_LEGS);
        check(&mut QuadrupedModel::pig(), &FOUR_LEGS);
        check(&mut SheepModel::new(), &FOUR_LEGS);
    }

    #[test]
    fn chicken_names() {
        check(
            &mut ChickenModel::new(),
            &[
                "head",
                "body",
                "bill",
                "chin",
                "right_leg",
                "left_leg",
                "right_wing",
                "left_wing",
            ],
        );
    }

    #[test]
    fn wolf_names() {
        check(
            &mut WolfModel::new(),
            &[
                "head", "body", "mane", "tail", "leg1", "leg2", "leg3", "leg4",
            ],
        );
    }

    #[test]
    fn enderman_names() {
        check(
            &mut EndermanModel::new(),
            &[
                "head",
                "headwear",
                "body",
                "left_arm",
                "right_arm",
                "left_leg",
                "right_leg",
            ],
        );
    }

    #[test]
    fn villager_names() {
        check(
            &mut VillagerModel::villager(),
            &["head", "nose", "body", "arms", "right_leg", "left_leg"],
        );
    }

    #[test]
    fn iron_golem_names() {
        check(&mut IronGolemModel::new(), &BIPED);
    }
}
