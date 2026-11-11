// Copyright Open Universe. Все права защищены.

#include "OUClientNet.h"
#include "Sockets.h"
#include "SocketSubsystem.h"
#include "Interfaces/IPv4/IPv4Address.h"
#include "Interfaces/IPv4/IPv4Endpoint.h"
#include "Common/UdpSocketBuilder.h"
#include "Common/TcpSocketBuilder.h"

DEFINE_LOG_CATEGORY_STATIC(LogOUClient, Log, All);

namespace
{
	// --- Мини-кодек: varint (LEB128) и zigzag, как в Rust-протоколе ---

	void WriteVarint(TArray<uint8>& Out, uint64 Value)
	{
		while (Value >= 0x80)
		{
			Out.Add(static_cast<uint8>(Value) | 0x80);
			Value >>= 7;
		}
		Out.Add(static_cast<uint8>(Value));
	}

	void WriteSVarint(TArray<uint8>& Out, int64 Value)
	{
		// zigzag: −1 → 1, 1 → 2, чтобы мелкие модули занимали мало байт.
		const uint64 ZigZag = (static_cast<uint64>(Value) << 1) ^ static_cast<uint64>(Value >> 63);
		WriteVarint(Out, ZigZag);
	}

	bool ReadVarint(const TArray<uint8>& In, int32& Pos, uint64& Out)
	{
		uint64 Result = 0;
		int32 Shift = 0;
		while (Pos < In.Num())
		{
			const uint8 Byte = In[Pos++];
			Result |= static_cast<uint64>(Byte & 0x7F) << Shift;
			if ((Byte & 0x80) == 0)
			{
				Out = Result;
				return true;
			}
			Shift += 7;
			if (Shift >= 64)
			{
				return false;
			}
		}
		return false;
	}

	bool ReadSVarint(const TArray<uint8>& In, int32& Pos, int64& Out)
	{
		uint64 ZigZag = 0;
		if (!ReadVarint(In, Pos, ZigZag))
		{
			return false;
		}
		Out = static_cast<int64>(ZigZag >> 1) ^ -static_cast<int64>(ZigZag & 1);
		return true;
	}

	/** Чтение f32 в LE. */
	bool ReadF32(const TArray<uint8>& In, int32& Pos, float& Out)
	{
		if (Pos + 4 > In.Num())
		{
			return false;
		}
		uint32 Bits = 0;
		for (int32 i = 0; i < 4; ++i)
		{
			Bits |= static_cast<uint32>(In[Pos + i]) << (8 * i);
		}
		Pos += 4;
		FMemory::Memcpy(&Out, &Bits, sizeof(float));
		return true;
	}
}

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
		return false;
	}
	ServerEndpoint = ServerAddr;

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
	SendPacket(Body);

	State = EOUConnState::Connected;
	UE_LOG(LogOUClient, Log, TEXT("Подключение к %s:%d"), *ServerAddress, ServerPort);
	return true;
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

void UOUClientNet::SendPacket(const TArray<uint8>& Body)
{
	if (!Socket || !ServerEndpoint.IsValid())
	{
		return;
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
	Socket->SendTo(Packet.GetData(), Packet.Num(), BytesSent, *ServerEndpoint);
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
	// Читаем все доступные пакеты за кадр.
	TArray<uint8> Buffer;
	Buffer.SetNumUninitialized(1500);
	for (;;)
	{
		int32 BytesRead = 0;
		FInternetAddr From;
		if (!Socket->RecvFrom(Buffer.GetData(), Buffer.Num(), BytesRead, From) || BytesRead <= 0)
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
		if (Body.Num() > 0 && Body[0] == 5) // snapshot v2
		{
			FOUSnapshot Snap;
			if (ParseSnapshot(Body, Snap))
			{
				LatestSnapshot = Snap;
			}
		}
	}
}

bool UOUClientNet::ParseSnapshot(const TArray<uint8>& Body, FOUSnapshot& Out)
{
	int32 Pos = 1; // пропускаем тег
	uint64 Tick = 0, Ack = 0, Count = 0;
	if (!ReadVarint(Body, Pos, Tick))
	{
		return false;
	}
	if (!ReadVarint(Body, Pos, Ack))
	{
		return false;
	}
	if (Pos >= Body.Num())
	{
		return false;
	}
	const bool bBaseline = Body[Pos++] != 0;
	if (!ReadVarint(Body, Pos, Count))
	{
		return false;
	}

	Out.Tick = Tick;
	Out.AckInputSeq = static_cast<int32>(Ack);
	Out.bBaseline = bBaseline;
	Out.Entities.Reset(static_cast<int32>(Count));

	// Масштаб позиции на сервере: 1 единица провода = 0.0001 ед. мира.
	// В UE 1 юнит = 1 см, значит 1 ед. мира ≈ 1 м = 100 см.
	constexpr double WireScale = 1.0 / 10000.0; // провод → единицы мира
	constexpr double WorldToCm = 100.0;         // единицы мира → см UE

	for (uint64 i = 0; i < Count; ++i)
	{
		FOUEntityState E;
		uint64 Id = 0;
		if (!ReadVarint(Body, Pos, Id))
		{
			return false;
		}
		E.Id = static_cast<int32>(Id);

		int64 PX = 0, PY = 0, PZ = 0;
		if (!ReadSVarint(Body, Pos, PX) || !ReadSVarint(Body, Pos, PY) || !ReadSVarint(Body, Pos, PZ))
		{
			return false;
		}
		int64 VX = 0, VY = 0, VZ = 0;
		if (!ReadSVarint(Body, Pos, VX) || !ReadSVarint(Body, Pos, VY) || !ReadSVarint(Body, Pos, VZ))
		{
			return false;
		}
		if (!ReadF32(Body, Pos, E.Yaw) || !ReadF32(Body, Pos, E.Pitch))
		{
			return false;
		}

		const double S = WireScale * WorldToCm; // провод → см
		E.Position = FVector(PX * S, PY * S, PZ * S);
		E.Velocity = FVector(VX * S, VY * S, VZ * S);
		Out.Entities.Add(E);
	}
	return true;
}
