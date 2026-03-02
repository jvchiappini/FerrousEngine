content = open('crates/ferrous_app/src/runner.rs', encoding='utf-8').read()

# Find where about_to_wait starts
start = content.find('    fn about_to_wait(')

# Find the closing of the impl block after about_to_wait: '}\n}\n\n/// Desktop'
# The pattern is: fn ends with '    }', then impl block closes with '}\n', then blank line
end_marker = '}\n}\n\n/// Desktop entry point'
end = content.find(end_marker, start)
print(f'start={start}, end={end}')
print('Context:', repr(content[end:end+40]))

new_fn = """\
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // On wasm32: request_redraw() maps to requestAnimationFrame, which the
        // browser fires at the monitor refresh rate. Call it once; browser handles cadence.
        #[cfg(target_arch = "wasm32")]
        {
            if let Some(window) = &self.window {
                window.request_redraw();
            }
            return;
        }

        // Desktop: precise frame-budget + idle sleep logic.
        #[cfg(not(target_arch = "wasm32"))]
        {
            let Some(window) = &self.window else { return };

            let is_idle = if let Some(timeout) = self.config.idle_timeout {
                Instant::now()
                    .duration_since(self.last_action_time)
                    .as_secs_f32()
                    > timeout
            } else {
                false
            };

            if is_idle {
                event_loop.set_control_flow(ControlFlow::Wait);
                return;
            }

            if let Some(target_fps) = self.config.target_fps {
                let budget = Duration::from_secs_f64(1.0 / target_fps as f64);
                let next_frame = self.last_frame + budget;
                if Instant::now() >= next_frame {
                    window.request_redraw();
                } else {
                    event_loop.set_control_flow(ControlFlow::WaitUntil(next_frame));
                }
            } else {
                window.request_redraw();
            }
        }
    }
}"""

# Replace from start of fn to and including the closing '}' of the impl block
# end points to '}' of fn, then '\n}' closes impl
impl_close_end = end + 3  # '}\n}' = 3 chars
new_content = content[:start] + new_fn + content[impl_close_end:]
open('crates/ferrous_app/src/runner.rs', 'w', encoding='utf-8').write(new_content)
print('Done.')
