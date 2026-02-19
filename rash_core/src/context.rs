use crate::task::{Handlers, PendingHandlers, Tasks};
/// Context
///
/// Preserve state between executions
use crate::{error::Result, jinja::merge_option};
use minijinja::{Value, context};

/// Main data structure in `rash`.
/// It contents all [`task::Tasks`] with their [`vars::Vars`] to be executed
///
/// [`task::Tasks`]: ../task/type.Tasks.html
/// [`vars::Vars`]: ../vars/type.Vars.html
#[derive(Debug, Clone)]
pub struct Context<'a> {
    pub tasks: Tasks<'a>,
    vars: Value,
    /// Variables added to the context for the current scope of execution.
    scoped_vars: Option<Value>,
    handlers: Option<Handlers<'a>>,
    pending_handlers: PendingHandlers,
}

impl<'a> Context<'a> {
    /// Create a new context from [`task::Tasks`] and [`vars::Vars`].Error
    ///
    /// [`task::Tasks`]: ../task/type.Tasks.html
    /// [`vars::Vars`]: ../vars/type.Vars.html
    pub fn new(tasks: Tasks<'a>, vars: Value, scope_vars: Option<Value>) -> Self {
        Context {
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
        Context {
            tasks,
            vars,
            scoped_vars: scope_vars,
            handlers,
            pending_handlers: PendingHandlers::new(),
        }
    }

    fn execute_pending_handlers(&mut self) -> Result<()> {
        if self.handlers.is_none() || self.pending_handlers.is_empty() {
            return Ok(());
        }

        let handlers = self.handlers.as_ref().unwrap();
        let pending = self.pending_handlers.take_pending();

        for handler_name in &pending {
            if let Some(handler) = handlers.get(handler_name) {
                info!(target: "task",
                    "[handler:{}] - ",
                    handler.get_task().get_rendered_name(self.vars.clone())
                        .unwrap_or_else(|_| handler_name.to_string()),
                );
                let _ = handler.get_task().exec(self.vars.clone())?;
            } else {
                warn!("Handler '{}' not found", handler_name);
            }
        }

        Ok(())
    }

    /// Execute all Tasks in Context until empty.
    ///
    /// If this finishes correctly, it will return an [`error::Error`] with [`ErrorKind::EmptyTaskStack`].
    ///
    /// [`error::Error`]: ../error/struct.Error.html
    /// [`ErrorKind::EmptyTaskStack`]: ../error/enum.ErrorKind.html
    pub fn exec(&self) -> Result<Self> {
        let mut context = self.clone();

        while !context.tasks.is_empty() {
            let mut next_tasks = context.tasks.clone();
            let next_task = next_tasks.remove(0);

            info!(target: "task",
                "[{}:{}] - {} to go - ",
                context.vars.get_attr("rash")?.get_attr("path")?,
                next_task.get_rendered_name(context.vars.clone())
                    .unwrap_or_else(|_| next_task.get_module().get_name().to_owned()),
                context.tasks.len(),
            );

            let exec_result = next_task.exec(context.vars.clone())?;

            let changed = exec_result.get_changed();
            let flush_handlers = exec_result.is_flush_handlers();
            let new_vars = exec_result.take_vars();

            if changed && let Some(notify) = next_task.get_notify() {
                context.pending_handlers.notify(notify);
            }

            let vars = merge_option(context.vars.clone(), new_vars.clone());

            let scoped_vars_value = [context.scoped_vars, new_vars]
                .into_iter()
                .fold(context! {}, merge_option);
            let scoped_vars = if scoped_vars_value == context!() {
                None
            } else {
                Some(scoped_vars_value)
            };
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

    /// Get a reference to the variables
    pub fn get_vars(&self) -> &Value {
        &self.vars
    }

    /// Get a reference to the scoped variables
    pub fn get_scoped_vars(&self) -> Option<&Value> {
        self.scoped_vars.as_ref()
    }
}

/// [`task::Task`] parameters that can be set globally
#[derive(Debug)]
pub struct GlobalParams<'a> {
    pub r#become: bool,
    pub become_user: &'a str,
    pub check_mode: bool,
}

impl Default for GlobalParams<'_> {
    fn default() -> Self {
        GlobalParams {
            r#become: Default::default(),
            become_user: "root",
            check_mode: Default::default(),
        }
    }
}

#[cfg(test)]
use std::sync::LazyLock;

#[cfg(test)]
pub static GLOBAL_PARAMS: LazyLock<GlobalParams> = LazyLock::new(GlobalParams::default);
