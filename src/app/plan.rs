use super::*;

/// План, ожидающий решения пользователя на гейте.
pub(crate) struct PendingPlan {
    pub(crate) task: String,
    pub(crate) plan: String,
}

/// Какая фаза плана сейчас выполняется (для роутинга завершения рана).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PlanFlow {
    None,
    Planning { task: String },
    Executing,
}

/// Решение на гейте по содержимому инпута.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum PlanGateAction {
    Approve,
    Refine(String),
}

/// Пустой инпут — одобрить; непустой — доработать с замечанием.
pub(crate) fn plan_gate_action(input: &str) -> PlanGateAction {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        PlanGateAction::Approve
    } else {
        PlanGateAction::Refine(trimmed.to_string())
    }
}

impl App {
    /// Фаза 1: запустить read-only генерацию плана для новой задачи.
    pub(crate) fn start_plan(&mut self, task: String) {
        let context = recent_chat_context(&self.transcript, 40);
        let prompt = plan_prompt(&task, &context, self.lang);
        self.plan_flow = PlanFlow::Planning { task: task.clone() };
        self.run_provider_chat(format!("◆ {task}"), prompt, RunAccess::PlanReadonly, true);
        if self.running {
            self.status = self
                .lang
                .choose("составляю план...", "drafting plan...")
                .to_string();
        } else {
            // запуск не стартовал (занят/нет авторизации) — снять флаг фазы
            self.plan_flow = PlanFlow::None;
        }
    }

    /// Доработать показанный план по замечанию пользователя (снова фаза 1).
    pub(crate) fn refine_plan(&mut self, feedback: String) {
        let Some(pending) = self.pending_plan.take() else {
            return;
        };
        let context = recent_chat_context(&self.transcript, 40);
        let prompt = refine_prompt(&pending.task, &pending.plan, &feedback, &context, self.lang);
        self.plan_flow = PlanFlow::Planning { task: pending.task };
        self.run_provider_chat(
            format!("◆ {feedback}"),
            prompt,
            RunAccess::PlanReadonly,
            true,
        );
        if self.running {
            self.status = self
                .lang
                .choose("дорабатываю план...", "refining plan...")
                .to_string();
        } else {
            self.plan_flow = PlanFlow::None;
        }
    }

    /// Фаза 2: выполнить одобренный план с полным доступом.
    pub(crate) fn approve_plan(&mut self) {
        let Some(pending) = self.pending_plan.take() else {
            return;
        };
        let context = recent_chat_context(&self.transcript, 40);
        let prompt = execute_prompt(&pending.task, &pending.plan, &context, self.lang);
        self.plan_flow = PlanFlow::Executing;
        self.run_provider_chat(
            format!("▶ {}", self.lang.choose("Выполняю план", "Executing plan")),
            prompt,
            RunAccess::PlanExecute,
            false,
        );
        if self.running {
            self.status = self
                .lang
                .choose("выполняю план...", "executing plan...")
                .to_string();
        } else {
            self.plan_flow = PlanFlow::None;
        }
    }

    /// Отменить ожидающий план без выполнения.
    pub(crate) fn cancel_plan(&mut self) {
        self.pending_plan = None;
        self.plan_flow = PlanFlow::None;
        self.input.clear();
        self.cursor = 0;
        self.push_system(self.lang.choose("⏹ План отменён.", "⏹ Plan cancelled."));
        // Гейт закрыт — если в очереди ждут сообщения, возобновляем, а не морозим их.
        self.process_pending_messages();
    }

    /// Решение на гейте: Enter с пустым инпутом — выполнить, с текстом — доработать.
    pub(crate) fn submit_plan_gate(&mut self) {
        match plan_gate_action(&self.input) {
            PlanGateAction::Approve => {
                self.input.clear();
                self.cursor = 0;
                self.approve_plan();
            }
            PlanGateAction::Refine(feedback) => {
                self.input.clear();
                self.cursor = 0;
                self.refine_plan(feedback);
            }
        }
    }

    /// Активен ли гейт одобрения (план готов и ничего не выполняется).
    pub(crate) fn plan_gate_active(&self) -> bool {
        self.pending_plan.is_some() && !self.running
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_gate_action_routes_by_input() {
        assert_eq!(plan_gate_action("   "), PlanGateAction::Approve);
        assert_eq!(
            plan_gate_action("сделай проще"),
            PlanGateAction::Refine("сделай проще".to_string())
        );
    }
}
