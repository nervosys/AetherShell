//! Batch SGD for the selected-action squared temporal-difference loss.
//! Target-network values are detached; updates commit only after every value
//! and proposed parameter is finite.
use crate::{
    neural::{Activation, NeuralNetwork},
    rl::Experience,
};

pub(crate) fn train_batch(
    network: &mut NeuralNetwork,
    target_network: &NeuralNetwork,
    batch: &[Experience],
    learning_rate: f64,
    discount: f64,
) -> Result<(), &'static str> {
    if batch.is_empty() {
        return Ok(());
    }
    if !learning_rate.is_finite() || !discount.is_finite() {
        return Err("non-finite training rate");
    }
    validate(network)?;
    validate(target_network)?;
    let mut weight_gradients: Vec<Vec<Vec<f64>>> = network
        .layers
        .iter()
        .map(|layer| vec![vec![0.0; layer.input_size]; layer.output_size])
        .collect();
    let mut bias_gradients: Vec<Vec<f64>> = network
        .layers
        .iter()
        .map(|layer| vec![0.0; layer.output_size])
        .collect();
    for experience in batch {
        if experience.state.len() != network.layers[0].input_size
            || experience.next_state.len() != target_network.layers[0].input_size
            || !experience.reward.is_finite()
        {
            return Err("inconsistent training transition shape or reward");
        }
        finite(&experience.state)?;
        finite(&experience.next_state)?;
        let target = if experience.done {
            experience.reward
        } else {
            let values = target_network.forward(&experience.next_state);
            finite(&values)?;
            experience.reward + discount * values.into_iter().fold(f64::NEG_INFINITY, f64::max)
        };
        if !target.is_finite() {
            return Err("non-finite temporal-difference target");
        }
        let mut inputs = Vec::with_capacity(network.layers.len());
        let mut preactivations = Vec::with_capacity(network.layers.len());
        let mut outputs = Vec::with_capacity(network.layers.len());
        let mut current = experience.state.clone();
        for layer in &network.layers {
            if current.len() != layer.input_size {
                return Err("training state width differs from network");
            }
            finite(&current)?;
            let raw: Vec<f64> = layer
                .weights
                .iter()
                .zip(&layer.biases)
                .map(|(row, bias)| {
                    row.iter()
                        .zip(&current)
                        .fold(*bias, |sum, (weight, input)| sum + weight * input)
                })
                .collect();
            finite(&raw)?;
            let output = layer.forward(&current);
            finite(&output)?;
            inputs.push(current);
            preactivations.push(raw);
            outputs.push(output.clone());
            current = output;
        }
        if experience.action >= current.len() {
            return Err("training action is outside network outputs");
        }
        let mut upstream = vec![0.0; current.len()];
        upstream[experience.action] = current[experience.action] - target;
        for index in (0..network.layers.len()).rev() {
            let layer = &network.layers[index];
            let delta = if layer.activation == Activation::Softmax {
                let dot: f64 = upstream
                    .iter()
                    .zip(&outputs[index])
                    .map(|(g, p)| g * p)
                    .sum();
                upstream
                    .iter()
                    .zip(&outputs[index])
                    .map(|(g, p)| p * (g - dot))
                    .collect::<Vec<_>>()
            } else {
                upstream
                    .iter()
                    .zip(&preactivations[index])
                    .map(|(g, z)| g * layer.activation.derivative(*z))
                    .collect::<Vec<_>>()
            };
            finite(&delta)?;
            let mut next = vec![0.0; layer.input_size];
            for (row, &derivative) in delta.iter().enumerate() {
                bias_gradients[index][row] += derivative;
                for (column, &input) in inputs[index].iter().enumerate() {
                    weight_gradients[index][row][column] += derivative * input;
                    next[column] += layer.weights[row][column] * derivative;
                }
            }
            finite(&next)?;
            upstream = next;
        }
    }
    let scale = learning_rate / batch.len() as f64;
    let mut candidate = network.clone();
    for (index, layer) in candidate.layers.iter_mut().enumerate() {
        finite(&bias_gradients[index])?;
        for (row, weights) in layer.weights.iter_mut().enumerate() {
            finite(&weight_gradients[index][row])?;
            for (column, weight) in weights.iter_mut().enumerate() {
                *weight -= scale * weight_gradients[index][row][column];
            }
            finite(weights)?;
            layer.biases[row] -= scale * bias_gradients[index][row];
        }
        finite(&layer.biases)?;
    }
    *network = candidate;
    Ok(())
}

fn finite(values: &[f64]) -> Result<(), &'static str> {
    if values.iter().any(|value| !value.is_finite()) {
        Err("non-finite training arithmetic")
    } else {
        Ok(())
    }
}

fn validate(network: &NeuralNetwork) -> Result<(), &'static str> {
    if network.layers.is_empty() {
        return Err("training requires a nonempty network");
    }
    let mut previous = None;
    for layer in &network.layers {
        if layer.input_size == 0
            || layer.output_size == 0
            || layer.weights.len() != layer.output_size
            || layer.biases.len() != layer.output_size
            || layer
                .weights
                .iter()
                .any(|row| row.len() != layer.input_size)
            || previous.is_some_and(|width| width != layer.input_size)
        {
            return Err("inconsistent training network shape");
        }
        finite(&layer.biases)?;
        for row in &layer.weights {
            finite(row)?;
        }
        if let Activation::LeakyReLU(alpha) = layer.activation {
            finite(&[alpha])?;
        }
        previous = Some(layer.output_size);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn transition(state: Vec<f64>, action: usize, reward: f64, done: bool) -> Experience {
        Experience {
            next_state: state.clone(),
            state,
            action,
            reward,
            done,
        }
    }
    fn loss(network: &NeuralNetwork, batch: &[Experience]) -> f64 {
        batch
            .iter()
            .map(|e| 0.5 * (network.forward(&e.state)[e.action] - e.reward).powi(2))
            .sum::<f64>()
            / batch.len() as f64
    }
    #[test]
    fn gradients_match_central_differences_through_all_activations() {
        let batch = vec![
            transition(vec![0.2, 0.7], 0, 0.5, true),
            transition(vec![0.6, -0.4], 1, -0.3, true),
        ];
        for activation in [
            Activation::ReLU,
            Activation::Sigmoid,
            Activation::Tanh,
            Activation::Linear,
            Activation::LeakyReLU(0.1),
            Activation::Swish,
            Activation::Softmax,
        ] {
            let mut network =
                NeuralNetwork::feedforward("gradient", &[2, 3, 2], activation, Activation::Linear);
            let parameters = (0..network.param_count())
                .map(|i| 0.15 + i as f64 * 0.01)
                .collect::<Vec<_>>();
            network.set_params(&parameters);
            let mut updated = network.clone();
            train_batch(&mut updated, &network, &batch, 0.001, 0.9).unwrap();
            for (index, &after) in updated.get_params().iter().enumerate() {
                let mut plus = network.clone();
                let mut minus = network.clone();
                let mut p = parameters.clone();
                p[index] += 1e-6;
                plus.set_params(&p);
                p[index] -= 2e-6;
                minus.set_params(&p);
                let expected = (loss(&plus, &batch) - loss(&minus, &batch)) / 2e-6;
                let actual = (parameters[index] - after) / 0.001;
                assert!(
                    (actual - expected).abs() < 1e-7,
                    "{activation:?} parameter {index}: {actual} vs {expected}"
                );
            }
            assert!(loss(&updated, &batch) < loss(&network, &batch));
        }
    }
    #[test]
    fn targets_are_detached_and_terminal_transitions_do_not_bootstrap() {
        let mut network =
            NeuralNetwork::feedforward("policy", &[1, 2], Activation::ReLU, Activation::Linear);
        network.set_params(&[0.0, 0.0, 0.0, 0.0]);
        let mut target = network.clone();
        target.set_params(&[2.0, 5.0, 0.0, 0.0]);
        let original_target = target.get_params();
        train_batch(
            &mut network,
            &target,
            &[transition(vec![1.0], 0, 1.0, false)],
            0.1,
            0.5,
        )
        .unwrap();
        assert!((network.get_params()[0] - 0.35).abs() < 1e-12);
        assert!((network.get_params()[2] - 0.35).abs() < 1e-12);
        assert_eq!(network.get_params()[1], 0.0);
        assert_eq!(network.get_params()[3], 0.0);
        assert_eq!(target.get_params(), original_target);
        let mut terminal = transition(vec![1.0], 0, 1.0, true);
        terminal.next_state = vec![f64::MAX];
        network.set_params(&[0.0, 0.0, 0.0, 0.0]);
        train_batch(&mut network, &target, &[terminal], 0.1, 0.5).unwrap();
        assert!((network.get_params()[0] - 0.1).abs() < 1e-12);
    }
    #[test]
    fn arithmetic_failure_keeps_every_parameter_unchanged() {
        let mut network =
            NeuralNetwork::feedforward("overflow", &[1, 1], Activation::ReLU, Activation::Linear);
        network.set_params(&[1.0, 0.0]);
        let target = network.clone();
        let before = network.get_params();
        let batch = [transition(vec![f64::MAX], 0, -f64::MAX, true)];
        assert!(train_batch(&mut network, &target, &batch, 0.1, 0.9).is_err());
        assert_eq!(network.get_params(), before);
    }
}
