use crate::task::{Handlers, PendingHandlers, Tasks};
use crate::{error::Result, jinja::merge_option};
use clap::ValueEnum;
use minijinja::{Value, context};

#[derive(Debug, Clone)]
pub struct Context<'a> {
    pub tasks: Tasks<'a>,
    vars: Value,
    scoped_vars: Option<Value>,
    handlers: Option<Handlers<'a>>,
    pending_handlers: PendingHandlers,
}

impl<'a> Context<'a> {
    pub fn new(tasks: Tasks<'a>, vars: Value, scope_vars: Option<Value>) -> Self {
        Self {
            tasks,
            vars,
            scoped_vars: scope_vars,
            handlers: None,
            pending_handlers: PendingHandlers::new(),
        }
    }

    pub fn with_handlers(
        tasks: Tasks<'a>,
        vars: Value,
        scope_vars: Option<Value>,
        handlers: Option<Handlers<'a>>,
    ) -> Self {
        Self {
            tasks,
            vars,
            scoped_vars: scope_vars,
            handlers,
            pending_handlers: PendingHandlers::new(),
        }
    }

    fn task_display_name(task: &crate::task::Task<'_>, vars: Value) -> String {
        if task.get_no_log() {
            return "<redacted>".to_owned();
        }
        task.get_rendered_name(vars)
            .unwrap_or_else(|_| task.get_module().get_name().to_owned())
    }

    fn execute_pending_handlers(&mut self) -> Result<()> {
        if self.handlers.is_none() || self.pending_handlers.is_empty() {
            return Ok(());
        }

        let handlers = self.handlers.as_ref().unwrap();
        let pending = self.pending_handlers.take_pending();
        for handler_name in &pending {
            if let Some(handler) = handlers.get(handler_name) {
                let task = handler.get_task();
                info!(target: "task",
                    "[handler:{}] - ",
                    Self::task_display_name(task, self.vars.clone()),
                );
                let _ = task.exec(self.vars.clone())?;
            } else {
                warn!("Handler '{handler_name}' not found");
            }
        }
        Ok(())
    }

    pub fn exec(&self) -> Result<Self> {
        let mut context = self.clone();

        while !context.tasks.is_empty() {
            let mut next_tasks = context.tasks.clone();
            let next_task = next_tasks.remove(0);

            info!(target: "task",
                "[{}:{}] - {} to go - ",
                context.vars.get_attr("rash")?.get_attr("path")?,
                Self::task_display_name(&next_task, context.vars.clone()),
                context.tasks.len(),
            );

            let exec_result = next_task.exec(context.vars.clone())?;
            let changed = exec_result.get_changed();
            let failed = exec_result.get_failed();
            let flush_handlers = exec_result.is_flush_handlers();
            let new_vars = exec_result.take_vars();

            // An ignored failure remains `failed: true` for scripting decisions, but must not
            // trigger a handler merely because the underlying module reported a change.
            if changed
                && !failed
                && let Some(notify) = next_task.get_notify()
            {
                context.pending_handlers.notify(notify);
            }

            let vars = merge_option(context.vars.clone(), new_vars.clone());
            let scoped_vars_value = [context.scoped_vars, new_vars]
                .into_iter()
                .fold(context! {}, merge_option);
            let scoped_vars = (scoped_vars_value != context! {}).then_some(scoped_vars_value);

            context = Self {
                tasks: next_tasks,
                vars,
                scoped_vars,
                handlers: context.handlers,
                pending_handlers: context.pending_handlers,
            };

            if flush_handlers {
                context.execute_pending_handlers()?;
            }
        }

        context.execute_pending_handlers()?;
        Ok(context)
    }

    pub fn get_vars(&self) -> &Value {
        &self.vars
    }

    pub fn get_scoped_vars(&self) -> Option<&Value> {
        self.scoped_vars.as_ref()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum BecomeMethod {
    #[default]
    Syscall,
    Sudo,
}

impl std::str::FromStr for BecomeMethod {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "syscall" => Ok(Self::Syscall),
            "sudo" => Ok(Self::Sudo),
            _ => Err(format!(
                "Invalid become_method '{s}'. Valid options: syscall, sudo"
            )),
        }
    }
}

impl std::fmt::Display for BecomeMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Syscall => write!(f, "syscall"),
            Self::Sudo => write!(f, "sudo"),
        }
    }
}

#[derive(Debug)]
pub struct GlobalParams<'a> {
    pub r#become: bool,
    pub become_user: &'a str,
    pub become_method: BecomeMethod,
    pub become_exe: &'a str,
    pub become_password: Option<&'a str>,
    pub check_mode: bool,
}

impl Default for GlobalParams<'_> {
    fn default() -> Self {
        Self {
            r#become: false,
            become_user: "root",
            become_method: BecomeMethod::default(),
            become_exe: "sudo",
            become_password: None,
            check_mode: false,
        }
    }
}

#[cfg(test)]
use std::sync::LazyLock;

#[cfg(test)]
pub static GLOBAL_PARAMS: LazyLock<GlobalParams> = LazyLock::new(GlobalParams::default);
