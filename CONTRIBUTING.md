# Вклад в ForgePlan

До крупных изменений откройте issue с пользовательским сценарием и критериями
приёмки. Изменения расчётной семантики требуют обновления SCHEDULING.md,
engine version policy и эталонных тестов. Не копируйте чужой закрытый код.

Проверки: cargo fmt --all; cargo clippy --workspace --all-targets -- -D warnings;
cargo test --workspace. Файлы Microsoft Project для тестов должны быть созданы
автором вклада и не содержать конфиденциальных данных.

До внешних вкладов владелец проекта должен утвердить лицензионную стратегию
и правила прав на contributions. Текущий пакет распространяется по Apache-2.0.

Для изменений клиента повторите clippy/test с --features native-client; Linux GUI
проверяется через xvfb-run и tests/native_e2e.py. Платформенные зависимости и
границы acceptance описаны в docs/NATIVE_CLIENT.md и docs/PLATFORMS.md.

При работе с библиотечным API используйте Context7 по правилам AGENTS.md и
сверяйте результаты с закреплённой версией: dev/master docs могут отличаться.
