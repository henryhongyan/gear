#[cfg(assimp)]
extern crate assimp;

extern crate k;
extern crate nalgebra as na;
extern crate ncollide;
extern crate rand;
extern crate rrt;
extern crate urdf_rs;
#[macro_use]
extern crate log;

mod errors;
pub use errors::*;

use na::{Isometry3, Vector3, Real, Translation3, UnitQuaternion};
use ncollide::ncollide_geometry::query::Proximity;
use ncollide::query;
use ncollide::shape::{Shape, ShapeHandle, Cuboid, Ball, Cylinder, TriMesh, Compound};
use std::collections::HashMap;
use std::path::Path;

fn from_urdf_pose<T>(pose: &urdf_rs::Pose) -> Isometry3<T>
where
    T: Real,
{
    Isometry3::from_parts(
        Translation3::new(
            na::convert(pose.xyz[0]),
            na::convert(pose.xyz[1]),
            na::convert(pose.xyz[2]),
        ),
        UnitQuaternion::from_euler_angles(
            na::convert(pose.rpy[0]),
            na::convert(pose.rpy[1]),
            na::convert(pose.rpy[2]),
        ),
    )

}

#[cfg(assimp)]
pub fn load_mesh<P, T>(filename: P, scale: &[f64]) -> Result<TriMesh<na::Point3<T>>>
where
    P: AsRef<Path>,
    T: Real,
{
    let mut importer = Importer::new();
    importer.pre_transform_vertices(|x| x.enable = true);
    importer.collada_ignore_up_direction(true);
    let file_string = filename.as_ref().to_str().ok_or(
        "faild to get string from path",
    )?;
    Ok(convert_assimp_scene_to_ncollide_mesh(
        importer.read_file(file_string)?,
        scale,
    ))
}

#[cfg(assimp)]
fn convert_assimp_scene_to_ncollide_mesh<T>(
    scene: assimp::Scene,
    scale: &[f64],
) -> TriMesh<na::Point3<T>>
where
    T: Real,
{
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    let mut last_index: usize = 0;
    for mesh in scene.mesh_iter() {
        vertices.extend(mesh.vertex_iter().map(|v| {
            na::Point3::<T>::new(
                na::convert(v.x as f64 * scale[0]),
                na::convert(v.y as f64 * scale[1]),
                na::convert(v.z as f64 * scale[2]),
            )
        }));
        indices.extend(mesh.face_iter().filter_map(|f| if f.num_indices == 3 {
            Some(na::Point3::<usize>::new(
                f[0] as usize + last_index,
                f[1] as usize + last_index,
                f[2] as usize + last_index,
            ))
        } else {
            None
        }));
        last_index = vertices.len() as usize;
    }
    TriMesh::new(
        std::sync::Arc::new(vertices),
        std::sync::Arc::new(indices),
        None,
        None,
    )
}


#[cfg(not(assimp))]
pub fn load_mesh<P, T>(_filename: P, _scale: &[f64]) -> Result<TriMesh<na::Point3<T>>>
where
    P: AsRef<Path>,
    T: Real,
{
    Err(Error::from("mesh is not not supported"))
}

pub fn wrap_compound<T, S>(
    shape: S,
    origin: Isometry3<T>,
) -> Compound<na::Point3<T>, na::Isometry3<T>>
where
    T: Real,
    S: Shape<na::Point3<T>, na::Isometry3<T>>,
{
    let mut shapes = Vec::new();
    let handle = ShapeHandle::new(shape);
    shapes.push((origin, handle));
    Compound::new(shapes)
}

pub fn create_collision_model<T>(
    collision: &urdf_rs::Collision,
    base_dir: &Path,
) -> Option<Compound<na::Point3<T>, na::Isometry3<T>>>
where
    T: Real,
{
    let pose = from_urdf_pose(&collision.origin);
    match collision.geometry {
        urdf_rs::Geometry::Box { ref size } => {
            let cube = Cuboid::new(Vector3::new(
                na::convert(size[0] * 0.5),
                na::convert(size[1] * 0.5),
                na::convert(size[2] * 0.5),
            ));
            Some(wrap_compound(cube, pose))
        }
        urdf_rs::Geometry::Cylinder { radius, length } => {
            Some(wrap_compound(
                Cylinder::new(
                    na::convert(length * 0.5),
                    na::convert(radius),
                ),
                pose,
            ))
        }
        urdf_rs::Geometry::Sphere { radius } => {
            Some(wrap_compound(Ball::new(na::convert(radius)), pose))
        }
        urdf_rs::Geometry::Mesh {
            ref filename,
            scale,
        } => {
            let replaced_filename = urdf_rs::utils::expand_package_path(filename, base_dir);
            let path = Path::new(&replaced_filename);
            if !path.exists() {
                error!("{} not found", replaced_filename);
                return None;
            }
            if let Ok(mesh) = load_mesh(path, &scale) {
                Some(wrap_compound(mesh, pose))
            } else {
                None
            }
        }
    }
}


pub struct CollisionChecker<T>
where
    T: Real,
{
    name_collision_model_map: HashMap<String, Compound<na::Point3<T>, na::Isometry3<T>>>,
    pub prediction: T,
}

impl<T> CollisionChecker<T>
where
    T: Real,
{
    pub fn new(urdf_robot: &urdf_rs::Robot, base_dir: &Path, prediction: T) -> Self {
        let mut collisions = HashMap::new();
        for l in &urdf_robot.links {
            if let Some(col) = create_collision_model(&l.collision, base_dir) {
                collisions.insert(l.name.to_string(), col);
            }
        }
        CollisionChecker {
            name_collision_model_map: collisions,
            prediction: prediction,
        }
    }
    pub fn get_colliding_link_names(
        &self,
        robot: &k::LinkTree<T>,
        target_shape: &Shape<na::Point3<T>, na::Isometry3<T>>,
        target_pose: &na::Isometry3<T>,
    ) -> Vec<String> {
        let mut names = Vec::new();
        for (trans, link_name) in
            robot.calc_link_transforms().iter().zip(
                robot.iter_link().map(
                    |link| link.name.clone(),
                ),
            )
        {
            match self.name_collision_model_map.get(&link_name) {
                Some(obj) => {
                    // TODO: only first shape is supported
                    let ctct = query::proximity(
                        &(trans * obj.shapes()[0].0),
                        &*obj.shapes()[0].1,
                        target_pose,
                        target_shape,
                        self.prediction,
                    );
                    if ctct != Proximity::Disjoint {
                        names.push(link_name);
                    }
                }
                None => {
                    println!("{} not found", link_name);
                }
            }
        }
        names
    }
}

pub fn generate_random_joint_angles_from_limits<T>(limits: &Vec<Option<k::Range<T>>>) -> Vec<T>
where
    T: Real + rand::Rand,
{
    limits
        .iter()
        .map(|range| match *range {
            Some(ref range) => (range.max - range.min) * na::convert(rand::random()) + range.min,
            None => (rand::random::<T>() - na::convert(0.5)) * na::convert(2.0),
        })
        .collect()
}

pub struct CollisionAvoidJointPathPlanner {
    robot: k::LinkTree<f64>,
    pub collision_checker: CollisionChecker<f64>,
    pub step_length: f64,
    pub max_try: usize,
}

impl CollisionAvoidJointPathPlanner {
    pub fn new(robot: k::LinkTree<f64>, collision_checker: CollisionChecker<f64>) -> Self {
        CollisionAvoidJointPathPlanner {
            robot: robot,
            collision_checker: collision_checker,
            step_length: 0.05,
            max_try: 1000,
        }
    }
    pub fn is_feasible(
        &mut self,
        joint_angles: &[f64],
        target_shape: &Shape<na::Point3<f64>, na::Isometry3<f64>>,
        target_pose: &na::Isometry3<f64>,
    ) -> bool {
        self.set_joint_angles(joint_angles).unwrap();
        self.get_colliding_link_names(target_shape, target_pose)
            .is_empty()
    }
    pub fn set_joint_angles(
        &mut self,
        joint_angles: &[f64],
    ) -> std::result::Result<(), k::JointError> {
        self.robot.set_joint_angles(joint_angles)
    }
    pub fn get_joint_angles(&self) -> Vec<f64> {
        self.robot.get_joint_angles()
    }
    pub fn get_colliding_link_names(
        &self,
        target_shape: &Shape<na::Point3<f64>, na::Isometry3<f64>>,
        target_pose: &na::Isometry3<f64>,
    ) -> Vec<String> {
        self.collision_checker.get_colliding_link_names(
            &self.robot,
            target_shape,
            target_pose,
        )
    }
    pub fn plan(
        &mut self,
        goal_angles: &[f64],
        target_shape: &Shape<na::Point3<f64>, na::Isometry3<f64>>,
        target_pose: &na::Isometry3<f64>,
    ) -> std::result::Result<Vec<Vec<f64>>, String> {
        let initial_angles = self.get_joint_angles();
        let limits = self.robot
            .iter_for_joints_link()
            .map(|link| link.joint.limits.clone())
            .collect();
        let step_length = self.step_length;
        let max_try = self.max_try;
        if !self.is_feasible(&initial_angles, target_shape, target_pose) ||
            !self.is_feasible(&goal_angles, target_shape, target_pose)
        {
            return Err("Initial or Goal is colliding".to_owned());
        }
        rrt::dual_rrt_connect(
            &initial_angles,
            goal_angles,
            |angles: &[f64]| self.is_feasible(angles, target_shape, target_pose),
            || generate_random_joint_angles_from_limits(&limits),
            step_length,
            max_try,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let urdf_robot = urdf_rs::read_file("sample.urdf").unwrap();
        let checker = CollisionChecker::new(&urdf_robot, Path::new("./"), 0.05);

        let target = Cuboid::new(Vector3::new(0.5, 0.5, 0.5));
        let target_pose = Isometry3::new(Vector3::new(0.0, 0.0, -0.5), na::zero());

        let robot = k::urdf::create_tree::<f32>(&urdf_robot);
        let names = checker.get_collision_link_names(&robot, &target, &target_pose);
        println!("{:?}", names);
    }
}
