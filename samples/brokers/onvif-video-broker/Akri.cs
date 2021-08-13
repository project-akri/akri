using System;
using System.IO;
using System.Linq;
using System.Net;
using System.Net.Http;
using System.Text;
using System.Xml;
using System.Xml.XPath;

namespace Akri 
{
    public static class Akri 
    {
		private static string PostSoapRequest(String requestUri, String action, String soapMessage)
		{
			var request = (HttpWebRequest) WebRequest.CreateDefault(new Uri(requestUri));
			request.ContentType = "application/soap+xml; charset=utf-8";
			request.Method = HttpMethod.Post.ToString();
			request.Headers.Add("SOAPAction", action);
			using (var stream = new StreamWriter(request.GetRequestStream(), Encoding.UTF8))
			{
				stream.Write(soapMessage);
			}

			Console.WriteLine($"[Akri] ONVIF request {requestUri} {action}");
			using (WebResponse requestResponse = request.GetResponse())
			{
				using (StreamReader responseReader = new StreamReader(requestResponse.GetResponseStream()))
				{
					return responseReader.ReadToEnd();  
				}  
			}  			
		}

		private const String MEDIA_WSDL = "http://www.onvif.org/ver10/media/wsdl";
		private const String DEVICE_WSDL = "http://www.onvif.org/ver10/device/wsdl";
		private const String GET_SERVICE_SOAP = @"<soap:Envelope xmlns:soap=""http://www.w3.org/2003/05/soap-envelope"" xmlns:wsdl=""http://www.onvif.org/ver10/device/wsdl""><soap:Header/><soap:Body><wsdl:GetServices /></soap:Body></soap:Envelope>";
		private const String GET_PROFILES_SOAP = @"<soap:Envelope xmlns:soap=""http://www.w3.org/2003/05/soap-envelope"" xmlns:wsdl=""http://www.onvif.org/ver10/media/wsdl""><soap:Header/><soap:Body><wsdl:GetProfiles/></soap:Body></soap:Envelope>";
		private const String GET_STREAMING_URI_SOAP_TEMPLATE = @"<soap:Envelope xmlns:soap=""http://www.w3.org/2003/05/soap-envelope"" xmlns:wsdl=""http://www.onvif.org/ver10/media/wsdl"" xmlns:sch=""http://www.onvif.org/ver10/schema""><soap:Header/><soap:Body><wsdl:GetStreamUri><wsdl:StreamSetup><sch:Stream>RTP-Unicast</sch:Stream><sch:Transport><sch:Protocol>RTSP</sch:Protocol></sch:Transport></wsdl:StreamSetup><wsdl:ProfileToken>{0}</wsdl:ProfileToken></wsdl:GetStreamUri></soap:Body></soap:Envelope>";

		private static string GetMediaUrl(String device_service_url)
		{
			var servicesResult = PostSoapRequest(
				device_service_url,
				String.Format("{0}/{1}", DEVICE_WSDL, "GetService"),
				GET_SERVICE_SOAP
			);
			var document = new XPathDocument(new XmlTextReader(new StringReader(servicesResult)));
			var navigator = document.CreateNavigator();
			var xpath = String.Format("//*[local-name()='GetServicesResponse']/*[local-name()='Service' and *[local-name()='Namespace']/text() ='{0}']/*[local-name()='XAddr']/text()", MEDIA_WSDL);
			var media_url = navigator.SelectSingleNode(xpath).ToString();
			Console.WriteLine($"[Akri] ONVIF media url {media_url}");
			return media_url;
		}

		private static string GetProfile(String media_url)
		{
			var servicesResult = PostSoapRequest(
				media_url,
				String.Format("{0}/{1}", MEDIA_WSDL, "GetProfiles"),
				GET_PROFILES_SOAP
			);
			var document = new XPathDocument(new XmlTextReader(new StringReader(servicesResult)));
			var navigator = document.CreateNavigator();
			var xpath = String.Format("//*[local-name()='GetProfilesResponse']/*[local-name()='Profiles']/@token");
			var profileNodesIterator = navigator.Select(xpath);
			var profiles = (from XPathNavigator @group in profileNodesIterator select @group.Value).ToList();
			profiles.Sort();
			foreach (var p in profiles) {
				Console.WriteLine($"[Akri] ONVIF profile list contains: {p}");
			}
			// randomly choose first profile
			var profile = profiles.First();
			Console.WriteLine($"[Akri] ONVIF profile list {profile}");
			return profile;
		}

		private static string GetStreamingUri(String media_url, String profile_token)
		{
			var soapMessage = String.Format(GET_STREAMING_URI_SOAP_TEMPLATE, profile_token);
			var servicesResult = PostSoapRequest(
				media_url,
				String.Format("{0}/{1}", MEDIA_WSDL, "GetStreamUri"),
				soapMessage
			);
			var document = new XPathDocument(new XmlTextReader(new StringReader(servicesResult)));
			var navigator = document.CreateNavigator();
			var xpath = String.Format("//*[local-name()='GetStreamUriResponse']/*[local-name()='MediaUri']/*[local-name()='Uri']/text()");
			var profileNodesIterator = navigator.Select(xpath);
			var streaming_uri_list = (from XPathNavigator @group in profileNodesIterator select @group.Value).ToList();
			foreach (var u in streaming_uri_list) {
				Console.WriteLine($"[Akri] ONVIF streaming uri list contains: {u}");
			}
			// randomly choose first profile
			var streaming_uri = streaming_uri_list.First();
			Console.WriteLine($"[Akri] ONVIF streaming uri {streaming_uri}");
			return streaming_uri;
		}

        public static string GetRtspUrl()
        {
			var device_service_url = Environment.GetEnvironmentVariable("ONVIF_DEVICE_SERVICE_URL");
			if (string.IsNullOrEmpty(device_service_url))
			{
				throw new ArgumentNullException("ONVIF_DEVICE_SERVICE_URL undefined");
			}

			var media_url = GetMediaUrl(device_service_url);
			var profile = GetProfile(media_url);
			var streaming_url = GetStreamingUri(media_url, profile);
			return streaming_url;
        }

    }
}
