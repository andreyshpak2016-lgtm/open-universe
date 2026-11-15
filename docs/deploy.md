# Развёртывание Open Universe в Kubernetes

Единый шард состоит из нескольких **сервисов**, каждый — отдельный процесс с
своими ресурсами. Так мир живёт в нескольких узлах, а вход, чат и рынок можно
масштабировать независимо.

## Сервисы

| Сервис | Роль | Как масштабируется |
|---|---|---|
| `ou-auth` (`crates/auth`) | Выдаёт и проверяет билеты сессии | Deployment, 2+ реплики |
| `ou-netd` (`crates/server`) | Авторитетный шард: симуляция зон | StatefulSet, N узлов |
| рынок | В том же процессе `ou-netd` (единый шард) | по нагрузке зон |

Узлы шарда делят **зоны** (звёздные системы) детерминированно — consistent
hashing по кольцу виртуальных узлов (`crates/shard`). При росте/падении узлов
переезжает лишь малая доля зон, миграция игрока при пересечении границы зон —
через `ShardRouter::route`.

## Файлы

- `deploy/k8s/open-universe.yaml` — namespace, ConfigMap, StatefulSet шарда,
  Deployment auth, сервисы (headless + внешний UDP-LoadBalancer).
- `deploy/k8s/autoscaling.yaml` — HPA для узлов шарда и auth.

## Запуск (пример для minikube/кластера)

```bash
kubectl apply -f deploy/k8s/open-universe.yaml
kubectl apply -f deploy/k8s/autoscaling.yaml
kubectl -n open-universe get pods,svc
```

Внешний адрес игры — `ou-netd-entry` (UDP 7777). Клиент подключается к нему,
шард сам направляет игрока в узел-владелец его зоны.

## Локальная проверка шардирования (без кластера)

```bash
# Узел 1 из 3: слушает свой адрес, владеет только своими зонами.
server/target/debug/ou-netd.exe 127.0.0.1:7777 120 world --shard 1 --nodes 1,2,3
```

В HUD клиента появится строка `шард N` (или `шард N→M`, если зону обслуживает
другой узел) — это и есть подтверждение работы шардирования.

## Образы

Образы `openuniverse/ou-netd` и `openuniverse/ou-auth` собираются в CI
(`.github/workflows/ci.yml`) из `server/`. Секрет `OU_SECRET` подписывает
билеты сессии и **обязан совпадать** у auth и у узлов шарда.
