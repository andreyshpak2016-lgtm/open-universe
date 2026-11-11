// Copyright Open Universe. Все права защищены.

#include "OUClientNet.h"
#include "OUWire.h"
#include "Sockets.h"
#include "SocketSubsystem.h"
#include "Interfaces/IPv4/IPv4Address.h"
#include "Interfaces/IPv4/IPv4Endpoint.h"
#include "Common/UdpSocketBuilder.h"
#include "Common/TcpSocketBuilder.h"

DEFINE_LOG_CATEGORY_STATIC(LogOUClient, Log, All);

UOUClientNet::UOUClientNet()
{
	PrimaryComponentTick.bCanEverTick = true;
	PrimaryComponentTick.bStartWithTickEnabled = true;
}

void UOUClientNet::BeginPlay()
{
	Super::BeginPlay();
	Connect();
}

void UOUClientNet::EndPlay(const EEndPlayReason::Type Reason)
{
	Disconnect();
	Super::EndPlay(Reason);
}

bool UOUClientNet::Connect()
{
	if (Socket)
	{
		return true;
	}
	State = EOUConnState::Connecting;

	ISocketSubsystem* SocketSub = ISocketSubsystem::Get(PLATFORM_SOCKETSUBSYSTEM);
	if (!SocketSub)
	{
		State = EOUConnState::Error;
		UE_LOG(LogOUClient, Error, TEXT("Нет подсистемы сокетов"));
		return false;
	}

	// Клиентский UDP-сокет на любом свободном порту.
	Socket = FUdpSocketBuilder(TEXT("OUClient"))
		.AsNonBlocking()
		.Build();
	if (!Socket)
	{
		State = EOUConnState::Error;
		UE_LOG(LogOUClient, Error, TEXT("Не удалось создать сокет"));
		return false;
	}

	// Адрес сервера.
	TSharedPtr<FInternetAddr> ServerAddr = SocketSub->CreateInternetAddr();
	bool bValid = false;
	ServerAddr->SetIp(*ServerAddress, bValid);
	ServerAddr->SetPort(ServerPort);
	if (!bValid)
	{
		State = EOUConnState::Error;
		UE_LOG(LogOUClient, Error, TEXT("Неверный адрес сервера: %s"), *ServerAddress);
		// Освобождаем сокет, чтобы повторный Connect() не считал себя подключённым.
		Disconnect();
		return false;
	}
	ServerEndpoint = ServerAddr;

	if (!SendHello())
	{
		Disconnect();
		return false;
	}

	// Пока не пришёл ответ (welcome/снимок), состояние — «подключение».
	State = EOUConnState::Connecting;
	UE_LOG(LogOUClient, Log, TEXT("Приветствие отправлено на %s:%d"), *ServerAddress, ServerPort);
	return true;
}

bool UOUClientNet::SendHello()
{
	if (!Socket || !ServerEndpoint.IsValid())
	{
		return false;
	}
	// Приветствие (тег 1): [tag][protocol u16][name len u16][name].
	TArray<uint8> Body;
	Body.Add(1);
	Body.Add(1); // protocol lo
	Body.Add(0); // protocol hi
	const FTCHARToUTF8 NameUtf8(*PlayerName);
	const int32 NameLen = NameUtf8.Length();
	Body.Add(static_cast<uint8>(NameLen & 0xFF));
	Body.Add(static_cast<uint8>((NameLen >> 8) & 0xFF));
	for (int32 i = 0; i < NameLen; ++i)
	{
		Body.Add(static_cast<uint8>(NameUtf8.Get()[i]));
	}
	return SendPacket(Body);
}

void UOUClientNet::Disconnect()
{
	if (Socket)
	{
		Socket->Close();
		ISocketSubsystem::Get(PLATFORM_SOCKETSUBSYSTEM)->DestroySocket(Socket);
		Socket = nullptr;
	}
	State = EOUConnState::Disconnected;
}

bool UOUClientNet::SendPacket(const TArray<uint8>& Body)
{
	if (!Socket || !ServerEndpoint.IsValid())
	{
		return false;
	}
	// Пакет: [token u64][len u32 LE][body].
	TArray<uint8> Packet;
	Packet.Reserve(8 + 4 + Body.Num());
	for (int32 i = 0; i < 8; ++i)
	{
		Packet.Add(static_cast<uint8>((static_cast<uint64>(SessionToken) >> (8 * i)) & 0xFF));
	}
	const uint32 Len = static_cast<uint32>(Body.Num());
	for (int32 i = 0; i < 4; ++i)
	{
		Packet.Add(static_cast<uint8>((Len >> (8 * i)) & 0xFF));
	}
	Packet.Append(Body);

	int32 BytesSent = 0;
	return Socket->SendTo(Packet.GetData(), Packet.Num(), BytesSent, *ServerEndpoint) && BytesSent > 0;
}

void UOUClientNet::SendInput(const FOUInputFrame& Input)
{
	if (!IsConnected())
	{
		return;
	}
	// Кадр (тег 2): [tag][count u16][seq u32][throttle i16][yaw i16][pitch i16][fire u8]
	TArray<uint8> Body;
	Body.Add(2);
	Body.Add(1); // count = 1
	Body.Add(0);

	const uint32 Seq = static_cast<uint32>(Input.Seq);
	for (int32 i = 0; i < 4; ++i)
	{
		Body.Add(static_cast<uint8>((Seq >> (8 * i)) & 0xFF));
	}
	// Квантование −1..1 в i16 (масштаб 32767, как на сервере).
	auto Quantize = [](float V) -> int16
	{
		const float Clamped = FMath::Clamp(V, -1.f, 1.f);
		return static_cast<int16>(FMath::RoundToInt(Clamped * 32767.f));
	};
	auto AddI16 = [&Body](int16 V)
	{
		Body.Add(static_cast<uint8>(V & 0xFF));
		Body.Add(static_cast<uint8>((V >> 8) & 0xFF));
	};
	AddI16(Quantize(Input.Throttle));
	AddI16(Quantize(Input.Yaw));
	AddI16(Quantize(Input.Pitch));
	Body.Add(Input.bFire ? 1 : 0);

	SendPacket(Body);
}

void UOUClientNet::Poll()
{
	if (!Socket)
	{
		return;
	}
	// Адрес отправителя создаём через фабрику: FInternetAddr абстрактный.
	ISocketSubsystem* SocketSub = ISocketSubsystem::Get(PLATFORM_SOCKETSUBSYSTEM);
	TSharedRef<FInternetAddr> From = SocketSub->CreateInternetAddr();

	// Читаем все доступные пакеты за кадр.
	TArray<uint8> Buffer;
	Buffer.SetNumUninitialized(1500);
	for (;;)
	{
		int32 BytesRead = 0;
		if (!Socket->RecvFrom(Buffer.GetData(), Buffer.Num(), BytesRead, *From) || BytesRead <= 0)
		{
			break;
		}
		// Проверяем токен: первые 8 байт.
		if (BytesRead < 12)
		{
			continue;
		}
		uint64 Token = 0;
		for (int32 i = 0; i < 8; ++i)
		{
			Token |= static_cast<uint64>(Buffer[i]) << (8 * i);
		}
		if (Token != static_cast<uint64>(SessionToken))
		{
			continue;
		}
		// Тело: с 12-го байта.
		TArray<uint8> Body;
		Body.Append(Buffer.GetData() + 12, BytesRead - 12);
		if (Body.Num() == 0)
		{
			continue;
		}
		if (Body[0] == OUWire::TagWelcome) // приветствие: наш id
		{
			int32 Entity = 0;
			if (OUWire::DecodeWelcome(Body.GetData(), static_cast<size_t>(Body.Num()), Entity))
			{
				OwnEntityId = Entity;
				State = EOUConnState::Connected;
				UE_LOG(LogOUClient, Log, TEXT("Сервер принял: моя сущность = %d"), Entity);
			}
		}
		else if (Body[0] == OUWire::TagSnapshotV2) // snapshot v2
		{
			FOUSnapshot Snap;
			if (ParseSnapshot(Body, Snap))
			{
				LatestSnapshot = Snap;
				++SnapshotsReceived;
				State = EOUConnState::Connected;
			}
		}
	}
}

bool UOUClientNet::ParseSnapshot(const TArray<uint8>& Body, FOUSnapshot& Out)
{
	// Разбор делегирован OUWire — тому же коду, который проверен на реальном
	// пакете сервера стендом `client/testdata/test_wire.cpp`. Дублирования нет,
	// а значит клиент и тест не могут разойтись.
	OUWire::Snapshot Snap;
	if (!OUWire::DecodeSnapshot(Body.GetData(), static_cast<size_t>(Body.Num()), Snap))
	{
		return false;
	}

	Out.Tick = Snap.Tick;
	Out.AckInputSeq = Snap.AckInputSeq;
	Out.bBaseline = Snap.Baseline;
	Out.Entities.Reset(static_cast<int32>(Snap.Entities.size()));
	for (const OUWire::EntityState& E : Snap.Entities)
	{
		FOUEntityState S;
		S.Id = E.Id;
		S.Position = FVector(E.X, E.Y, E.Z);
		S.Velocity = FVector(E.VX, E.VY, E.VZ);
		S.Yaw = E.Yaw;
		S.Pitch = E.Pitch;
		Out.Entities.Add(S);
	}
	return true;
}
