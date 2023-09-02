using Camera;
using Grpc.Core;
using Microsoft.AspNetCore.Authentication;
using Microsoft.AspNetCore.Hosting;
using Microsoft.Extensions.Hosting;
using OpenCvSharp;
using Prometheus;
using System;
using System.Collections;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text.RegularExpressions;
using System.Threading;
using System.Threading.Tasks;

namespace FrameServer
{
	public class CameraService : Camera.Camera.CameraBase
	{
		public override Task<NotifyResponse> GetFrame(
			NotifyRequest request, ServerCallContext context)
		{
			// Mask credential information in Program.RtspUrl to prevent the credential info shown in console output
			var rtspUrlWithMaskedCredential = RtspUrlHelper.GetMaskedCredentialUrl(Program.RtspUrl);
			byte[] frame = null;
			lock (Program.Frames)
			{
				if (Program.Frames.Any())
				{
					frame = Program.Frames.Pop();
					Program.JobsInQueue.Set(Program.Frames.Count);
				}

				if (frame == null)
				{
					Console.WriteLine("No frame available for {0}", rtspUrlWithMaskedCredential);
				}
				else
				{
					Console.WriteLine("Sending frame for {0}, Q size: {1}", rtspUrlWithMaskedCredential, Program.Frames.Count);
				}
			}

			return Task.FromResult(new NotifyResponse
			{
				Camera = Program.RtspUrl,
				Frame = (frame == null ? Google.Protobuf.ByteString.Empty : Google.Protobuf.ByteString.CopyFrom(frame))
			});
		}
	}

	// based on https://stackoverflow.com/questions/14101310/limit-the-size-of-a-generic-collection
	public class LimitedSizeStack<T> : LinkedList<T>
	{
		private readonly int _maxSize;
		public LimitedSizeStack(int maxSize)
		{
			_maxSize = maxSize;
		}

		public void Push(T item)
		{
			this.AddFirst(item);

			if (this.Count > _maxSize)
				this.RemoveLast();
		}

		public T Pop()
		{
			var item = this.First.Value;
			this.RemoveFirst();
			return item;
		}
	}

	public static class RtspUrlHelper {
		public static string GetMaskedCredentialUrl(string rtspUrl) {
			const string rtspPrefix = "rtsp://";
			var maskedRtspUrl = rtspUrl;
			if (rtspUrl.StartsWith(rtspPrefix)) {
				var atPos = rtspUrl.IndexOf('@', rtspPrefix.Length);
				if (atPos != -1) {
					maskedRtspUrl = rtspUrl.Substring(atPos);
					maskedRtspUrl = String.Format("{0}----:----{1}", rtspPrefix, maskedRtspUrl);
				}
			}
			return maskedRtspUrl;
		}
	}

	class Program
	{
		public static Task FrameTask;
		public static string RtspUrl;
		public static LimitedSizeStack<byte[]> Frames;

		static void Main(string[] args)
		{
			var frameBufferSizeSetting = Environment.GetEnvironmentVariable("FRAME_BUFFER_SIZE");
			int frameBufferSize =
				string.IsNullOrEmpty(frameBufferSizeSetting) ? 2 : int.Parse(frameBufferSizeSetting);
			Frames = new LimitedSizeStack<byte[]>(frameBufferSize);
			if (Frames == null) {
				throw new ArgumentNullException("Unable to create Frames");
			}

			RtspUrl = Environment.GetEnvironmentVariable("RTSP_URL");
			if (string.IsNullOrEmpty(RtspUrl)) {
				RtspUrl = Akri.Akri.GetRtspUrl();
			}
			if (string.IsNullOrEmpty(RtspUrl))
			{
				throw new ArgumentNullException("Unable to find RTSP URL");
			}

			CamerasCounter.Inc();

			FrameTask = Task.Run(() => Process(RtspUrl));

			var metricServer = new KestrelMetricServer(port: 8080);
			metricServer.Start();

			CreateHostBuilder(args).Build().Run();
		}

		public static IHostBuilder CreateHostBuilder(string[] args) =>
			Host.CreateDefaultBuilder(args)
		.ConfigureWebHostDefaults(webBuilder =>
		{
			webBuilder.UseStartup<Startup>();
		});

		public static readonly Gauge JobsInQueue = Metrics.CreateGauge(
			"cached_frames",
			"Number of cached camera frames.");

		private static readonly Counter CamerasCounter = Metrics.CreateCounter(
			"cameras",
			"Number of connected cameras.");

		private static readonly Counter CameraDisconnectCounter = Metrics.CreateCounter(
			"camera_disconnects",
			"Number of times camera connection had to be restablished.");

		static void Process(string videoPath)
		{
			// Mask credential information in videoPath to prevent the credential info shown in console output
			var videoPathWithMaskedCredential = RtspUrlHelper.GetMaskedCredentialUrl(videoPath);
			Console.WriteLine($"[VideoProcessor] Processing RTSP stream: {videoPathWithMaskedCredential}");

			while (true)
			{
				var capture = new VideoCapture(videoPath);
				Console.WriteLine("Ready " + capture.IsOpened());

				using (var image = new Mat()) // Frame image buffer
				{
					// Loop while we can read an image (aka: image.Empty is not true)
					while (capture.Read(image) && !image.Empty())
					{
						lock (Frames)
						{
							var imageBytes = image.ToBytes();
							Frames.Push(imageBytes);
							JobsInQueue.Set(Frames.Count);
							Console.WriteLine("Adding frame from {0}, Q size: {1}, frame size: {2}", videoPathWithMaskedCredential, Program.Frames.Count, imageBytes.Length);
						}
					}
				}

				CameraDisconnectCounter.Inc();
				Console.WriteLine($"[VideoProcessor] Reopening");
			}
		}
	}
}

