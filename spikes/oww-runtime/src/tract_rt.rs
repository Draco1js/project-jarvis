use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Result;
use tract_onnx::prelude::*;

use crate::{PlanCache, Runtime};

type Plan = std::sync::Arc<TypedRunnableModel>;

pub struct TractModel(PlanCache<Plan>);

impl TractModel {
    pub fn new(path: PathBuf) -> Self {
        Self(PlanCache { path, plans: HashMap::new() })
    }
}

impl Runtime for TractModel {
    fn run(&mut self, input: &[f32], shape: &[usize]) -> Result<Vec<f32>> {
        let cache = &mut self.0;
        if !cache.plans.contains_key(shape) {
            let plan = tract_onnx::onnx()
                .model_for_path(&cache.path)?
                .with_input_fact(0, f32::fact(shape).into())?
                .into_optimized()?
                .into_runnable()?;
            cache.plans.insert(shape.to_vec(), plan);
        }
        let plan = &cache.plans[shape];
        let t = Tensor::from_shape(shape, input)?;
        let out = plan.run(tvec!(t.into()))?;
        Ok(out[0].to_plain_array_view::<f32>()?.iter().copied().collect())
    }
}
