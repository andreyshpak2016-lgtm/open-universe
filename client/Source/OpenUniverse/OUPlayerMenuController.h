// Copyright Open Universe. Все права защищены.

#pragma once

#include "CoreMinimal.h"
#include "GameFramework/PlayerController.h"
#include "OUGameTypes.h"
#include "OUHealthTypes.h"
#include "OUPlayerMenuController.generated.h"

class UOUClientNet;

/** Экран стартового меню. */
UENUM(BlueprintType)
enum class EOUMenuScreen : uint8
{
	/** Ввод позывного (персонаж). */
	Character UMETA(DisplayName = "Персонаж"),
	/** Выбор сервера и подключение. */
	Server UMETA(DisplayName = "Сервер"),
	/** Меню уже закрыто, идёт игра. */
	InGame UMETA(DisplayName = "Игра")
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

	/** Проверить доступность серверов (лёгкий handshake по UDP). */
	UFUNCTION(BlueprintCallable, Category = "Open Universe|Menu")
	void PingServers();

	/** Прочитать status.json с сайта (сервер+клиент) — асинхронно. */
	UFUNCTION(BlueprintCallable, Category = "Open Universe|Menu")
	void FetchHealth();

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

	/** Курсор мыши показывать в меню, прятать в игре. */
	void SetMenuCursor(bool bShow);

protected:
	// --- ввод ---
	void OnLeftClick();
	void OnTextInput(const FString& Text);
	void OnBackspace();

private:
	/** Подключиться к выбранному серверу. */
	void ConnectToSelected();

	/** Установить статус (цвет подсказки задаётся знаком). */
	void SetConnectStatus(const FString& Text);

	/** Сетевой компонент на корабле игрока. */
	UPROPERTY(Transient)
	TObjectPtr<UOUClientNet> Net = nullptr;

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
