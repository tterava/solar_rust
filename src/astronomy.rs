//                 bodies.Add(new CelestialBody("Earth", 5.97237E24, new Vector3D(149598023, 0, 0), new Vector3D(0, 29780, 0), 6371.0, 1E7, Brushes.Blue));
//                 bodies.Add(new CelestialBody("Moon", 7.342E22, new Vector3D(149598023 + 384399, 0, 0), new Vector3D(0, 29780 + 1022, 0), 1737.1, 1E7, Brushes.White));
//                 bodies.Add(new CelestialBody("Mercury", 3.3011E23, new Vector3D(57909050, 0, 0), new Vector3D(0, 47362, 0), 2439.7, 1E7, Brushes.Red));
//                 bodies.Add(new CelestialBody("Venus", 4.8675E24, new Vector3D(-108208E3, 0, 0), new Vector3D(0, -35020, 0), 6051.8, 1E7, Brushes.Green));
//                 bodies.Add(new CelestialBody("Mars", 6.4171E24, new Vector3D(0, 1.523679 * AU, 0), new Vector3D(-24077, 0, 0), 3389.5, 1E7, Brushes.OrangeRed));
//                 bodies.Add(new CelestialBody("Jupiter", 1.8986E27, new Vector3D(0, 5.20260 * AU, 0), new Vector3D(-13070, 0, 0), 71492, 1E6, Brushes.GhostWhite));
//                 bodies.Add(new CelestialBody("Europa", 5.799844E22, new Vector3D(0, 5.20260 * AU + 670900, 0), new Vector3D(-13070 - 13740, 0, 0), 1560.8, 1E6, Brushes.LightYellow));
//                 bodies.Add(new CelestialBody("Saturn", 5.6836E26, new Vector3D(9.554909 * AU, 0, 0), new Vector3D(0, 9690, 0), 58232, 1E6, Brushes.Gray));
//                 bodies.Add(new CelestialBody("Uranus", 8.6810E25, new Vector3D(0, -19.2184 * AU, 0), new Vector3D(6800, 0, 0), 25362, 1E8, Brushes.Cyan));
//                 bodies.Add(new CelestialBody("Neptune", 1.0243E26, new Vector3D(0, -30.110387 * AU, 0), new Vector3D(5430, 0, 0), 24622, 1E8, Brushes.Blue));
//                 bodies.Add(new CelestialBody("Monster", 6E30, new Vector3D(90 * AU, 4 * AU, 0), new Vector3D(-11000, 0, 0), 10E5, 50, Brushes.Bisque));

use crate::vector::Vector4d;

#[derive(Debug, Clone)]
pub struct AstronomicalObject {
    pub name: String,
    pub mass: f64,
    pub position: Vector4d,
    pub velocity: Vector4d,
    pub radius: f64,
    pub magnification: f64,
    pub color: [u8; 3],
}

impl AstronomicalObject {
    pub fn default() -> Vec<AstronomicalObject> {
        vec![
            AstronomicalObject {
                name: "Sun".to_string(),
                mass: 1.98855E30,
                position: Vector4d::default(),
                velocity: Vector4d::default(),
                radius: 695700.0,
                magnification: 20.0,
                color: [255, 255, 0],
            },
            AstronomicalObject {
                name: "Earth".to_string(),
                mass: 5.97237E24,
                position: Vector4d {
                    data: [149598023.0, 0.0, 0.0, 1.0],
                },
                velocity: Vector4d {
                    data: [0.0, 29780.0, 0.0, 1.0],
                },
                radius: 6371.0,
                magnification: 1.0E7,
                color: [0, 0, 255],
            },
        ]
    }
}
