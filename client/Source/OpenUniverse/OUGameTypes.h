// Copyright Open Universe. Все права защищены.

#pragma once

#include "CoreMinimal.h"
#include "OUGameTypes.generated.h"

/**
 * Профиль персонажа игрока (создаётся в стартовом меню).
 */
USTRUCT(BlueprintType)
struct FOUCharacterProfile
{
	GENERATED_BODY()

	/** Позывной (он же логин пилота). */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Open Universe|Menu")
	FString Name = TEXT("pilot");
};

/**
 * Сервер, к которому можно подключиться (список в меню).
 */
USTRUCT(BlueprintType)
struct FOUServerInfo
{
	GENERATED_BODY()

	/** Название для меню. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Open Universe|Menu")
	FString Label;

	/** Адрес (IP или домен). */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Open Universe|Menu")
	FString Address = TEXT("127.0.0.1");

	/** UDP-порт. */
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Open Universe|Menu")
	int32 Port = 7777;

	/** Статус доступности (заполняется проверкой; пока не проверен — неизвестно). */
	UPROPERTY(BlueprintReadOnly, Category = "Open Universe|Menu")
	bool bOnline = false;

	FOUServerInfo() {}
	FOUServerInfo(const FString& InLabel, const FString& InAddress, int32 InPort)
		: Label(InLabel), Address(InAddress), Port(InPort) {}
};
