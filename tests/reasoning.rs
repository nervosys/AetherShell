//! Tests for advanced reasoning functionality

use std::collections::HashMap;

use aether_shell::ai::{MultiModalContent, MultiModalMessage};
use aether_shell::tui::agents::Modality;
use aether_shell::tui::reasoning::*;

#[test]
fn test_reasoning_engine_creation() {
    let engine = ReasoningEngine::new();
    assert!(!engine.strategies.is_empty());
    assert_eq!(engine.planning_horizon, 20);
    assert_eq!(engine.confidence_threshold, 0.75);
    assert!(engine.knowledge_base.facts.is_empty());
    assert!(engine.knowledge_base.experiences.is_empty());
}

#[test]
fn test_knowledge_base_operations() {
    let mut kb = KnowledgeBase::new();

    let fact = Fact {
        id: uuid::Uuid::new_v4(),
        content: "Test fact content".to_string(),
        confidence: 0.9,
        modalities: vec![Modality::Text],
        source: "test_source".to_string(),
        timestamp: chrono::Utc::now(),
    };

    kb.add_fact(fact.clone());
    assert_eq!(kb.facts.len(), 1);
    assert!(kb.facts.contains_key(&fact.id.to_string()));

    let rule = Rule {
        id: uuid::Uuid::new_v4(),
        condition: "IF temperature > 30".to_string(),
        action: "THEN suggest cooling".to_string(),
        confidence: 0.8,
        applicable_modalities: vec![Modality::Text],
    };

    kb.add_rule(rule.clone());
    assert_eq!(kb.rules.len(), 1);
    assert_eq!(kb.rules[0].condition, rule.condition);

    let pattern = Pattern {
        id: uuid::Uuid::new_v4(),
        description: "Text to image pattern".to_string(),
        input_pattern: vec![Modality::Text],
        output_pattern: vec![Modality::Image],
        success_rate: 0.85,
        examples: vec![],
    };

    kb.add_pattern(pattern.clone());
    assert_eq!(kb.patterns.len(), 1);
    assert_eq!(kb.patterns[0].description, pattern.description);
}

#[tokio::test]
async fn test_reasoning_engine_main_method() {
    let mut engine = ReasoningEngine::new();

    let goal = PlanningGoal {
        description: "Analyze this text input".to_string(),
        input_data: MultiModalMessage {
            role: "user".to_string(),
            content: vec![MultiModalContent {
                text: Some("Test input for analysis".to_string()),
                image_url: None,
                audio_url: None,
                video_url: None,
                image_data: None,
                audio_data: None,
                video_data: None,
            }],
        },
        desired_output: GoalSpecification {
            output_modalities: vec![Modality::Text],
            quality_requirements: HashMap::new(),
            success_criteria: vec!["Generate coherent response".to_string()],
        },
        constraints: vec![],
        deadline: None,
    };

    let result = engine.reason(&goal).await;
    assert!(result.is_ok());

    let response = result.unwrap();
    assert!(!response.content.is_empty());
    assert_eq!(response.role, "assistant");
}

#[test]
fn test_reasoning_session_creation() {
    let goal = PlanningGoal {
        description: "Complex multi-step analysis".to_string(),
        input_data: MultiModalMessage {
            role: "user".to_string(),
            content: vec![MultiModalContent {
                text: Some("Complex input".to_string()),
                image_url: None,
                audio_url: None,
                video_url: None,
                image_data: None,
                audio_data: None,
                video_data: None,
            }],
        },
        desired_output: GoalSpecification {
            output_modalities: vec![Modality::Text],
            quality_requirements: HashMap::new(),
            success_criteria: vec!["Analyze thoroughly".to_string()],
        },
        constraints: vec![],
        deadline: None,
    };

    let session = ReasoningSession {
        id: uuid::Uuid::new_v4(),
        goal,
        current_strategy: ReasoningStrategy::TreeOfThought {
            branching_factor: 3,
            max_depth: 5,
            pruning_threshold: 0.7,
        },
        reasoning_chain: vec![],
        confidence_scores: vec![],
        intermediate_results: vec![],
        status: SessionStatus::Planning,
    };

    assert_eq!(session.status, SessionStatus::Planning);
    assert!(session.reasoning_chain.is_empty());
}

#[test]
fn test_modality_fusion_concepts() {
    let fusion_weights = HashMap::from([
        (Modality::Text, 0.5),
        (Modality::Image, 0.3),
        (Modality::Audio, 0.2),
    ]);

    assert_eq!(fusion_weights.len(), 3);
    assert_eq!(fusion_weights[&Modality::Text], 0.5);
    assert_eq!(fusion_weights[&Modality::Image], 0.3);
    assert_eq!(fusion_weights[&Modality::Audio], 0.2);
}

#[test]
fn test_planning_goal_creation() {
    let goal = PlanningGoal {
        description: "Multi-level planning task".to_string(),
        input_data: MultiModalMessage {
            role: "user".to_string(),
            content: vec![MultiModalContent {
                text: Some("Complex planning input".to_string()),
                image_url: None,
                audio_url: None,
                video_url: None,
                image_data: None,
                audio_data: None,
                video_data: None,
            }],
        },
        desired_output: GoalSpecification {
            output_modalities: vec![Modality::Text],
            quality_requirements: HashMap::new(),
            success_criteria: vec!["Plan successfully".to_string()],
        },
        constraints: vec![],
        deadline: None,
    };

    assert_eq!(goal.description, "Multi-level planning task");
    assert!(goal.constraints.is_empty());
    assert!(goal.deadline.is_none());
}

#[test]
fn test_constraint_creation() {
    let constraint = Constraint {
        constraint_type: ConstraintType::Quality,
        description: "Critical analysis requirement".to_string(),
        severity: ConstraintSeverity::Hard,
    };

    assert_eq!(constraint.constraint_type, ConstraintType::Quality);
    assert_eq!(constraint.severity, ConstraintSeverity::Hard);
    assert_eq!(constraint.description, "Critical analysis requirement");
}

#[test]
fn test_reasoning_step_structure() {
    let step = ReasoningStep {
        step_id: uuid::Uuid::new_v4(),
        description: "Test reasoning step".to_string(),
        input: MultiModalMessage {
            role: "user".to_string(),
            content: vec![MultiModalContent {
                text: Some("Test input for reasoning".to_string()),
                image_url: None,
                audio_url: None,
                video_url: None,
                image_data: None,
                audio_data: None,
                video_data: None,
            }],
        },
        output: Some(MultiModalMessage {
            role: "assistant".to_string(),
            content: vec![MultiModalContent {
                text: Some("Test reasoning output".to_string()),
                image_url: None,
                audio_url: None,
                video_url: None,
                image_data: None,
                audio_data: None,
                video_data: None,
            }],
        }),
        confidence: 0.85,
        reasoning_type: ReasoningType::Deduction,
        execution_time: 150.0,
    };

    assert_eq!(step.description, "Test reasoning step");
    assert_eq!(step.reasoning_type, ReasoningType::Deduction);
    assert!(step.output.is_some());
    assert_eq!(step.confidence, 0.85);
}

#[test]
fn test_goal_specification_creation() {
    let goal_spec = GoalSpecification {
        output_modalities: vec![Modality::Text, Modality::Image],
        quality_requirements: HashMap::from([
            ("accuracy".to_string(), 0.9),
            ("completeness".to_string(), 0.8),
        ]),
        success_criteria: vec![
            "Generate coherent response".to_string(),
            "Include relevant details".to_string(),
        ],
    };

    assert_eq!(goal_spec.output_modalities.len(), 2);
    assert_eq!(goal_spec.quality_requirements.len(), 2);
    assert_eq!(goal_spec.success_criteria.len(), 2);
    assert_eq!(goal_spec.quality_requirements["accuracy"], 0.9);
}

#[test]
fn test_execution_plan_creation() {
    let plan = ExecutionPlan {
        id: uuid::Uuid::new_v4(),
        steps: vec![
            PlanStep {
                id: uuid::Uuid::new_v4(),
                description: "Step 1: Analyze input".to_string(),
                agent_requirements: AgentRequirements {
                    required_capabilities: vec!["text_analysis".to_string()],
                    preferred_capabilities: vec!["nlp".to_string()],
                    minimum_confidence: 0.8,
                    resource_requirements: HashMap::from([
                        ("memory".to_string(), 1024.0),
                        ("cpu".to_string(), 0.5),
                    ]),
                },
                input_modalities: vec![Modality::Text],
                output_modalities: vec![Modality::Text],
                estimated_time: 2.5,
                confidence: 0.9,
                criticality: StepCriticality::Critical,
            },
            PlanStep {
                id: uuid::Uuid::new_v4(),
                description: "Step 2: Generate response".to_string(),
                agent_requirements: AgentRequirements {
                    required_capabilities: vec!["text_generation".to_string()],
                    preferred_capabilities: vec![],
                    minimum_confidence: 0.7,
                    resource_requirements: HashMap::new(),
                },
                input_modalities: vec![Modality::Text],
                output_modalities: vec![Modality::Text, Modality::Image],
                estimated_time: 3.0,
                confidence: 0.85,
                criticality: StepCriticality::Important,
            },
        ],
        dependencies: HashMap::new(),
        estimated_duration: 5.5,
        confidence: 0.875,
        fallback_plans: vec![FallbackPlan {
            trigger_condition: "Primary agent unavailable".to_string(),
            alternative_steps: vec![],
            confidence: 0.6,
        }],
    };

    assert_eq!(plan.steps.len(), 2);
    assert_eq!(plan.estimated_duration, 5.5);
    assert!(!plan.fallback_plans.is_empty());
}

#[test]
fn test_reasoning_session() {
    let session = ReasoningSession {
        id: uuid::Uuid::new_v4(),
        goal: create_test_planning_goal("Session test goal"),
        current_strategy: ReasoningStrategy::ChainOfThought {
            max_steps: 5,
            step_confidence_threshold: 0.8,
        },
        reasoning_chain: vec![],
        confidence_scores: vec![0.9, 0.85, 0.92],
        intermediate_results: vec![],
        status: SessionStatus::Planning,
    };

    assert_eq!(session.confidence_scores.len(), 3);
    assert_eq!(session.status, SessionStatus::Planning);
    assert!(session.reasoning_chain.is_empty());
}

#[test]
fn test_task_outcome_variants() {
    let success_outcome = TaskOutcome::Success {
        result: MultiModalMessage {
            role: "assistant".to_string(),
            content: vec![],
        },
        confidence: 0.9,
        execution_time: 2.5,
    };

    let failure_outcome = TaskOutcome::Failure {
        error_message: "Network timeout".to_string(),
        failure_point: "Step 3: API call".to_string(),
    };

    let partial_outcome = TaskOutcome::PartialSuccess {
        result: MultiModalMessage {
            role: "assistant".to_string(),
            content: vec![],
        },
        missing_aspects: vec!["image_generation".to_string()],
        confidence: 0.7,
    };

    // Test that outcomes can be matched
    match success_outcome {
        TaskOutcome::Success { confidence, .. } => assert_eq!(confidence, 0.9),
        _ => panic!("Expected success outcome"),
    }

    match failure_outcome {
        TaskOutcome::Failure { error_message, .. } => assert_eq!(error_message, "Network timeout"),
        _ => panic!("Expected failure outcome"),
    }

    match partial_outcome {
        TaskOutcome::PartialSuccess {
            missing_aspects, ..
        } => {
            assert_eq!(missing_aspects.len(), 1);
            assert_eq!(missing_aspects[0], "image_generation");
        }
        _ => panic!("Expected partial success outcome"),
    }
}

#[test]
fn test_reasoning_type_variants() {
    let reasoning_types = vec![
        ReasoningType::Induction,
        ReasoningType::Deduction,
        ReasoningType::Abduction,
        ReasoningType::Analogy,
        ReasoningType::Causal,
        ReasoningType::Temporal,
        ReasoningType::Spatial,
        ReasoningType::Modal,
    ];

    assert_eq!(reasoning_types.len(), 8);

    // Test that types can be cloned and compared
    let deduction1 = ReasoningType::Deduction;
    let deduction2 = deduction1.clone();
    assert_eq!(deduction1, deduction2);

    assert_ne!(ReasoningType::Induction, ReasoningType::Deduction);
}

#[test]
fn test_constraint_types_and_severities() {
    let constraints = vec![
        Constraint {
            constraint_type: ConstraintType::Resource,
            description: "Memory limit".to_string(),
            severity: ConstraintSeverity::Hard,
        },
        Constraint {
            constraint_type: ConstraintType::Time,
            description: "Deadline constraint".to_string(),
            severity: ConstraintSeverity::Soft,
        },
        Constraint {
            constraint_type: ConstraintType::Ethical,
            description: "Privacy protection".to_string(),
            severity: ConstraintSeverity::Hard,
        },
    ];

    assert_eq!(constraints.len(), 3);
    assert_eq!(constraints[0].constraint_type, ConstraintType::Resource);
    assert_eq!(constraints[1].severity, ConstraintSeverity::Soft);
    assert_eq!(constraints[2].description, "Privacy protection");
}

#[tokio::test]
async fn test_experience_storage() {
    let mut engine = ReasoningEngine::new();

    let goal = create_test_planning_goal("Experience storage test");
    let reasoning_chain = vec![ReasoningStep {
        step_id: uuid::Uuid::new_v4(),
        description: "Initial analysis".to_string(),
        input: MultiModalMessage {
            role: "user".to_string(),
            content: vec![],
        },
        output: Some(MultiModalMessage {
            role: "assistant".to_string(),
            content: vec![],
        }),
        confidence: 0.9,
        reasoning_type: ReasoningType::Deduction,
        execution_time: 1.5,
    }];

    let result = MultiModalMessage {
        role: "assistant".to_string(),
        content: vec![],
    };

    // Test that we can create experience data structure without storing
    let experience = Experience {
        id: uuid::Uuid::new_v4(),
        task_description: goal.description.clone(),
        input_modalities: vec![Modality::Text],
        strategy_used: ReasoningStrategy::ChainOfThought {
            max_steps: 5,
            step_confidence_threshold: 0.7,
        },
        outcome: TaskOutcome::Success {
            result: result.clone(),
            confidence: 0.8,
            execution_time: 500.0,
        },
        lessons_learned: vec!["Test lesson learned".to_string()],
        timestamp: chrono::Utc::now(),
    };

    assert_eq!(experience.task_description, goal.description);
    assert!(!experience.lessons_learned.is_empty());
}

// Helper functions for creating test data

fn create_test_planning_goal(description: &str) -> PlanningGoal {
    PlanningGoal {
        description: description.to_string(),
        input_data: MultiModalMessage {
            role: "user".to_string(),
            content: vec![MultiModalContent {
                text: Some("Test input".to_string()),
                image_url: None,
                audio_url: None,
                video_url: None,
                image_data: None,
                audio_data: None,
                video_data: None,
            }],
        },
        desired_output: GoalSpecification {
            output_modalities: vec![Modality::Text],
            quality_requirements: HashMap::new(),
            success_criteria: vec!["Generate coherent response".to_string()],
        },
        constraints: vec![],
        deadline: None,
    }
}

fn create_multimodal_planning_goal() -> PlanningGoal {
    PlanningGoal {
        description: "Multi-modal analysis task".to_string(),
        input_data: MultiModalMessage {
            role: "user".to_string(),
            content: vec![
                MultiModalContent {
                    text: Some("Analyze this content".to_string()),
                    image_url: None,
                    audio_url: None,
                    video_url: None,
                    image_data: None,
                    audio_data: None,
                    video_data: None,
                },
                MultiModalContent {
                    text: None,
                    image_url: Some("https://example.com/image.jpg".to_string()),
                    audio_url: None,
                    video_url: None,
                    image_data: None,
                    audio_data: None,
                    video_data: None,
                },
            ],
        },
        desired_output: GoalSpecification {
            output_modalities: vec![Modality::Text, Modality::Image],
            quality_requirements: HashMap::from([("accuracy".to_string(), 0.9)]),
            success_criteria: vec!["Comprehensive multi-modal analysis".to_string()],
        },
        constraints: vec![],
        deadline: None,
    }
}
