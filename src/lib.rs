/*
Copyright 2017 Takashi Ogura

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
*/
//! # Motion Planning Library for Robotics
//!
//! Get the collision free trajectory of joint angles. `ncollide3d` is used to check the
//! collision between the robot and the environment.
//!
extern crate assimp;
extern crate k;
#[macro_use]
extern crate log;
extern crate nalgebra as na;
extern crate ncollide3d;
extern crate num_traits;
extern crate rand;
extern crate rrt;
extern crate trajectory;
extern crate urdf_rs;

mod errors;
pub use errors::*;

mod collision_checker;
pub use collision_checker::*;

mod funcs;
pub use funcs::*;

mod ik;
pub use ik::*;

mod path_planner;
pub use path_planner::*;

mod ik_planner;
pub use ik_planner::*;

// re-export k::IK modules
pub use k::{InverseKinematicsSolver, JacobianIKSolver, JacobianIKSolverBuilder};
