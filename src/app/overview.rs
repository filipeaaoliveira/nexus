use super::*;

// Legacy runtime compatibility lives here for now. The additive overview reducer
// in `crate::app::reducer::overview` is the future source of truth.
impl AppState {
    pub fn move_selection_down(&mut self) {
        let count = self.visible_agent_count();
        if count == 0 {
            return;
        }
        let max = count - 1;
        self.selected = Some(match self.selected {
            Some(s) if s < max => s + 1,
            Some(_) => 0, // wrap to top
            None => 0,
        });
    }

    pub fn move_selection_up(&mut self) {
        let count = self.visible_agent_count();
        if count == 0 {
            return;
        }
        let max = count - 1;
        self.selected = Some(match self.selected {
            Some(s) if s > 0 => s - 1,
            Some(_) => max, // wrap to bottom
            None => 0,
        });
    }

    pub fn cycle_sort_mode(&mut self) {
        self.sort_mode = self.sort_mode.next();
    }

    pub fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
        if !self.show_help {
            self.help_scroll = 0;
        }
    }

    pub fn scroll_help_up(&mut self) {
        self.help_scroll = self.help_scroll.saturating_sub(1);
    }

    pub fn scroll_help_down(&mut self) {
        let max = crate::ui::help::help_lines(true).len() as u16;
        self.help_scroll = self.help_scroll.saturating_add(1).min(max);
    }

    pub fn dismiss_overlay(&mut self) {
        self.show_help = false;
        self.help_scroll = 0;
        self.filter_mode = false;
        self.filter = None;
        self.filter_input.clear();
        self.rename_mode = false;
        self.rename_input.clear();
        self.clamp_selection();
    }

    pub fn start_filter(&mut self) {
        self.filter_mode = true;
        self.filter_input.clear();
        self.filter = None;
    }

    pub fn filter_push_char(&mut self, c: char) {
        self.filter_input.push(c);
        self.sync_filter();
    }

    pub fn filter_pop_char(&mut self) {
        self.filter_input.pop();
        self.sync_filter();
    }

    pub fn confirm_filter(&mut self) {
        self.filter_mode = false;
        // Keep filter active
        self.clamp_selection();
    }

    fn sync_filter(&mut self) {
        self.filter = if self.filter_input.is_empty() {
            None
        } else {
            Some(self.filter_input.clone())
        };
        self.clamp_selection();
    }

    pub fn start_rename(&mut self) {
        let Some((_real_idx, _)) = self.resolve_selected() else {
            return;
        };
        self.rename_mode = true;
        self.rename_input.clear();
    }

    pub fn rename_push_char(&mut self, c: char) {
        self.rename_input.push(c);
    }

    pub fn rename_pop_char(&mut self) {
        self.rename_input.pop();
    }

    pub fn confirm_rename(&mut self) {
        if let Some((real_idx, _id)) = self.resolve_selected() {
            if real_idx < self.agents.len() && !self.rename_input.is_empty() {
                self.agents[real_idx].alias = Some(self.rename_input.clone());
                self.mark_agents_dirty();
            }
        }
        self.rename_mode = false;
        self.rename_input.clear();
    }

    pub fn cancel_rename(&mut self) {
        self.rename_mode = false;
        self.rename_input.clear();
    }

    pub fn remove_exited(&mut self) {
        self.agents.retain(|a| a.status != PrimaryStatus::Exited);
        self.mark_agents_dirty();
        self.clamp_selection();
    }

    pub fn remove_selected(&mut self) {
        let (real_idx, _id) = match self.resolve_selected() {
            Some(v) => v,
            None => return,
        };
        self.agents.remove(real_idx);
        self.mark_agents_dirty();
        self.clamp_selection();
    }

    pub(super) fn clamp_selection(&mut self) {
        let count = self.visible_agent_count();
        if count == 0 {
            self.selected = None;
        } else if self.selected.is_none() {
            self.selected = Some(0);
        } else if let Some(idx) = self.selected {
            if idx >= count {
                self.selected = Some(count - 1);
            }
        }
    }

    pub(super) fn resolve_selected(&self) -> Option<(usize, AgentId)> {
        let vis_idx = self.selected?;
        self.with_visible_agent_indices(|indices| {
            let real_idx = *indices.get(vis_idx)?;
            let id = self.agents[real_idx].id.clone();
            Some((real_idx, id))
        })
    }

    pub(super) fn mark_agents_dirty(&self) {
        self.agents_version.set(self.agents_version.get() + 1);
    }

    pub(super) fn with_visible_agent_indices<R>(&self, f: impl FnOnce(&[usize]) -> R) -> R {
        self.refresh_visible_agent_cache();
        let cache = self.visible_agent_cache.borrow();
        f(&cache.indices)
    }

    fn refresh_visible_agent_cache(&self) {
        let current_version = self.agents_version.get();
        let current_filter = self.filter.clone();
        let current_sort_mode = self.sort_mode;
        let should_refresh = {
            let cache = self.visible_agent_cache.borrow();
            cache.version != current_version
                || cache.filter != current_filter
                || cache.sort_mode != Some(current_sort_mode)
        };

        if !should_refresh {
            return;
        }

        let indices = Self::compute_visible_agent_indices(
            &self.agents,
            current_filter.as_deref(),
            current_sort_mode,
        );
        *self.visible_agent_cache.borrow_mut() = VisibleAgentCache {
            version: current_version,
            filter: current_filter,
            sort_mode: Some(current_sort_mode),
            indices,
        };
    }

    fn compute_visible_agent_indices(
        agents: &[Agent],
        filter: Option<&str>,
        sort_mode: SortMode,
    ) -> Vec<usize> {
        let mut indices: Vec<usize> = agents
            .iter()
            .enumerate()
            .filter(|(_, agent)| {
                !(agent.id.tool() == AgentTool::Codex
                    && agent.id.is_session()
                    && agent.pid.is_none())
            })
            .map(|(idx, _)| idx)
            .collect();

        if let Some(filter) = filter {
            let filter = filter.to_lowercase();
            indices.retain(|&idx| Self::agent_matches_filter(&agents[idx], &filter));
        }

        match sort_mode {
            SortMode::Urgency => {
                indices.sort_by_key(|&idx| {
                    let agent = &agents[idx];
                    (std::cmp::Reverse(agent.normalized.status), agent.status)
                });
            }
            SortMode::Name => {
                indices.sort_by(|&left, &right| {
                    let a = &agents[left];
                    let b = &agents[right];
                    a.display_name()
                        .cmp(b.display_name())
                        .then_with(|| a.pid.cmp(&b.pid))
                        .then_with(|| a.id.session_id().cmp(&b.id.session_id()))
                });
            }
            SortMode::Elapsed => {
                indices.sort_by(|&left, &right| {
                    agents[left].started_at.cmp(&agents[right].started_at)
                });
            }
        }

        indices
    }

    pub(super) fn agent_matches_filter(agent: &Agent, filter: &str) -> bool {
        agent.display_name().to_lowercase().contains(filter)
            || agent.status.as_str().to_lowercase().contains(filter)
            || agent
                .normalized
                .status
                .as_str()
                .to_lowercase()
                .contains(filter)
            || agent
                .normalized
                .reason
                .as_ref()
                .map(|reason| reason.as_str().to_lowercase().contains(filter))
                .unwrap_or(false)
            || agent.cwd.to_string_lossy().to_lowercase().contains(filter)
            || agent
                .branch
                .as_deref()
                .map(|branch| branch.to_lowercase().contains(filter))
                .unwrap_or(false)
    }

    pub(super) fn visible_agent_count(&self) -> usize {
        self.with_visible_agent_indices(|indices| indices.len())
    }
}
