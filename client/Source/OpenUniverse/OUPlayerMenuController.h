// Copyright Open Universe. Все права защищены.

#pragma once

#include "CoreMinimal.h"
#include "GameFramework/PlayerController.h"
#include "OUGameTypes.h"
#include "OUHealthTypes.h"
#include "OUPlayerMenuController.generated.h"

class UOUClientNet;
class AOUShipPawn;

/** Экран стартового меню. */
UENUM(BlueprintType)
enum class EOUMenuScreen : uint8
{
	/** Ввод позывного (персонаж). */
	Character UMETA(DisplayName = "Персонаж"),
	/** Выбор сервера и подключение. */
	Server UMETA(DisplayName = "Сервер"),
	/** Меню уже закрыто, идёт игра. */
	InGame UMETA(DisplayName = "Игра"),
	/** Пауза-меню (ESC в игре): продолжить / настройки / отлучиться. */
	Pause UMETA(DisplayName = "Пауза"),
	/** Настройки (из пауза-меню). */
	Settings UMETA(DisplayName = "Настройки")
};

/**
 * Стартовое меню персонажа и выбора сервера.
 *
 * Рисуется через Canvas (без UMG-ассетов): сначала игрок вводит позывной,
 * затем выбирает сервер из списка (или вводит адрес вручную) и подключается.
 * После подключения меню закрывается и начинается игра.
 *
 * Сделано без ассетов намеренно: проект должен запускаться сразу после сборки,
 * без ручной настройки виджетов в редакторе.
 */
UCLASS()
class OPENUNIVERSE_API AOUPlayerMenuController : public APlayerController
{
	GENERATED_BODY()

public:
	AOUPlayerMenuController();

	/** Включён ли показ меню (иначе — игра). */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Menu")
	bool bMenuOpen = true;

	/** Открыто ли меню (для гейта ввода в корабле). */
	UFUNCTION(BlueprintPure, Category = "Open Universe|Menu")
	bool IsMenuOpen() const { return bMenuOpen; }

	/**
	 * Пауза-меню открыто (ESC в игре). Пока оно открыто, игра ставится на паузу,
	 * курсор свободен, а игровой ввод (ходьба/обзор) не проходит.
	 */
	UFUNCTION(BlueprintPure, Category = "Open Universe|Menu")
	bool IsPauseMenuOpen() const { return bPauseOpen; }

	/** Открыть пауза-меню (ESC в игре). */
	UFUNCTION(BlueprintCallable, Category = "Open Universe|Menu")
	void OpenPauseMenu();

	/** Закрыть пауза-меню и вернуться в игру. */
	UFUNCTION(BlueprintCallable, Category = "Open Universe|Menu")
	void ClosePauseMenu();

	/** Отлучиться от сервера: разорвать соединение и вернуться в стартовое меню. */
	UFUNCTION(BlueprintCallable, Category = "Open Universe|Menu")
	void LeaveServer();

	/** Нарисовать пауза-меню в канвасе (вызывает HUD). */
	void DrawPauseMenu(class UCanvas* InCanvas);

	/** Открыто ли пауза-меню. */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Menu")
	bool bPauseOpen = false;

	/** Текущий экран меню. */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Menu")
	EOUMenuScreen Screen = EOUMenuScreen::Character;

	/** Профиль персонажа. */
	UPROPERTY(BlueprintReadWrite, Category = "Open Universe|Menu")
	FOUCharacterProfile Profile;

	/** Список серверов. */
	UPROPERTY(BlueprintReadWrite, Category = "Open Universe|Menu")
	TArray<FOUServerInfo> Servers;

	/** Индекс выбранного сервера. */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Menu")
	int32 SelectedServer = 0;

	/** Адрес, введённый вручную (если игрок не хочет брать из списка). */
	UPROPERTY(BlueprintReadWrite, Category = "Open Universe|Menu")
	FString ManualAddress;

	/** Статус «сервер + клиент», прочитанный с сайта. */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Menu")
	FOUHealthStatus Health;

	/** Идёт ли сейчас подключение. */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Menu")
	bool bConnecting = false;

	/** Строка статуса подключения для меню. */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Menu")
	FString ConnectStatus;

	/** Диагностика: писали ли уже в лог о первом кадре меню. */
	bool bLoggedFirstDraw = false;

	/** Тестовый авто-выход выполнен (чтобы не повторять). */
	bool bAutoExitDone = false;

	/**
	 * Авто-подключение (для тестов и автоматики).
	 *
	 * Включается ключом командной строки `-OUAutoConnect`. Тогда меню само
	 * подставляет позывной, перебирает серверы из списка и подключается к
	 * **первому доступному** — тестам не нужно кликать мышью.
	 */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Menu")
	bool bAutoConnect = false;

	/** Позывной для авто-подключения (ключ `-OUName=Пилот`). */
	FString AutoConnectName;

	/**
	 * Тестовый авто-выход из корабля (ключ `-OUAutoExit`): после подключения
	 * автоматически выйти из корабля и пройтись. Нужен для проверки ходьбы
	 * без ручного нажатия F.
	 */
	bool bAutoExit = false;

	/** Сколько по времени ждать ответа от сервера при авто-подключении. */
	float AutoConnectTimeoutSec = 1.5f;

	/** Проверить доступность серверов (лёгкий handshake по UDP). */
	UFUNCTION(BlueprintCallable, Category = "Open Universe|Menu")
	void PingServers();

	/** Прочитать status.json с сайта (сервер+клиент) — асинхронно. */
	UFUNCTION(BlueprintCallable, Category = "Open Universe|Menu")
	void FetchHealth();

	/**
	 * Попробовать подключиться к серверу из списка по индексу.
	 * Возвращает false, если адрес некорректен (не из-за сети).
	 */
	UFUNCTION(BlueprintCallable, Category = "Open Universe|Menu")
	bool ConnectToServerIndex(int32 Index);

	/** Подключиться по конкретному адресу/порту (для ручного ввода и тестов). */
	UFUNCTION(BlueprintCallable, Category = "Open Universe|Menu")
	bool ConnectToAddress(const FString& Address, int32 Port);

protected:
	virtual void BeginPlay() override;
	virtual void SetupInputComponent() override;
	virtual void Tick(float DeltaSeconds) override;

	/** Ввод символов с клавиатуры: печатаем позывной, ловим Backspace/Enter. */
	virtual bool InputKey(const FInputKeyParams& Params) override;

public:
	/**
	 * Нарисовать меню в канвасе (вызывает HUD).
	 *
	 * Держим рисование открытым методом: `APlayerController` не имеет
	 * `DrawHUD` — его рисует `AHUD`. Так логика меню живёт в контроллере,
	 * а отрисовка вызывается из HUD.
	 */
	void DrawMenu(class UCanvas* InCanvas);

	/**
	 * Нарисовать меню строительства поверх игры (палитра блоков и подсказки).
	 * Вызывается из HUD, когда меню персонажа закрыто, а игрок в игре.
	 */
	void DrawBuildMenu(class UCanvas* InCanvas);

	/**
	 * Постоянный игровой HUD в духе Dual Universe: прицел, статусы в углах,
	 * хотбар внизу. Рисуется всегда, когда открыт игровой мир (меню закрыто).
	 */
	void DrawGameHud(class UCanvas* InCanvas);

	/**
	 * Окно рынка и инвентаря (клавиша M).
	 *
	 * Показывает кредиты, содержимое трюма и верх стакана по каждому предмету.
	 * Можно выставить ордер покупки/продажи по выбранному предмету — команда
	 * уходит на сервер, он авторитетно исполняет.
	 */
	void DrawMarket(class UCanvas* InCanvas);

	/** Открыто ли окно рынка. */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Economy")
	bool bMarketOpen = false;

	/** Имя предмета по id (дублирует серверный реестр). */
	static FString ItemName(int32 Item);

	/** Курсор мыши показывать в меню, прятать в игре. */
	void SetMenuCursor(bool bShow);

protected:
	// --- ввод ---
	void OnLeftClick();
	void OnTextInput(const FString& Text);
	void OnBackspace();

private:
	/** Подключиться к выбранному серверу (из списка или вручную). */
	void ConnectToSelected();

	/** Установить статус (цвет подсказки задаётся знаком). */
	void SetConnectStatus(const FString& Text);

	/** Запустить авто-подключение (перебор серверов). */
	void StartAutoConnect();

	/** Один шаг авто-подключения: попробовать следующий сервер. */
	void AutoConnectStep();

	/** Сетевой компонент на корабле игрока. */
	UPROPERTY(Transient)
	TObjectPtr<UOUClientNet> Net = nullptr;

	/** Корабль игрока — носитель сетевого компонента (игрок появляется персонажем). */
	UPROPERTY(Transient)
	TObjectPtr<AOUShipPawn> Ship = nullptr;

	/** Индекс текущего пробуемого сервера при авто-подключении. */
	int32 AutoConnectIndex = -1;

	/** Авто-подключение: ждём ли ответа от сервера. */
	bool bAwaitingWelcome = false;

	/** Авто-подключение: дедлайн ожидания (мир, сек). */
	float AutoConnectDeadline = 0.f;

	/** Гарантирует, что авто-подключение запущено один раз. */
	bool bAutoConnectStarted = false;

	/** Диагностика `-OUPauseMenu`: таймер до открытия пауза-меню. */
	float PauseShotTimer = 0.f;

	/** Найти сеть (после создания Pawn). */
	void ResolveNet();

	/** Печатный символ для клавиши (0 — если символ не буквенно-цифровой). */
	static TCHAR KeyToChar(const FKey& Key);

	/** Адрес сайта со status.json. */
	static const TCHAR* HealthUrl;

	/** Кнопки, отрисованные в прошлом кадре (для попадания мышью). */
	struct FButtonHit
	{
		FVector2D Pos;
		FVector2D Size;
		int32 Id; // смысл зависит от экрана
	};
	TArray<FButtonHit> Buttons;
};
