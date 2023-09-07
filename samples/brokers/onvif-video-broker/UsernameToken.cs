using System;
using System.Text;
using System.Security.Cryptography;

namespace Akri
{
	class UsernameToken {
		public string Username { get; }
		public string Password { get; }

		public UsernameToken(string username, string password)
		{
			this.Username = username;
			this.Password = password;
		}

		public string ToXml() {
			if (this.Username is null) {
				return "";
			}
			var password = this.Password?? "";

			var nonce = CalculateNonce();
			var created = DateTime.UtcNow.ToString("yyyy-MM-ddTHH:mm:ss.fffZ");
			var passwordDigest = CalculatePasswordDigest(nonce, created, password);
			return String.Format(SOAP_SECURITY_HEADER_TEMPLATE, this.Username, passwordDigest, nonce, created);
		}

		private const String SOAP_SECURITY_HEADER_TEMPLATE = @"<wsse:Security soap:mustUnderstand=""1"" xmlns:wsse=""http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-wssecurity-secext-1.0.xsd"">
			<wsse:UsernameToken xmlns:wsu=""http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-wssecurity-utility-1.0.xsd"">
				<wsse:Username>{0}</wsse:Username>
				<wsse:Password Type=""http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-username-token-profile-1.0#PasswordDigest"">{1}</wsse:Password>
				<wsse:Nonce EncodingType=""http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-soap-message-security-1.0#Base64Binary"">{2}</wsse:Nonce>
				<wsu:Created xmlns=""http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-wssecurity-utility-1.0.xsd"">{3}</wsu:Created>
			</wsse:UsernameToken>
		</wsse:Security>";

		private string CalculateNonce()
		{
			var buffer = new byte[16];
			using (var r = RandomNumberGenerator.Create())
			{
				r.GetBytes(buffer);
			}
			return Convert.ToBase64String(buffer);
		}

		private string CalculatePasswordDigest(string nonceStr, string created, string password)
		{
			var nonce = Convert.FromBase64String(nonceStr);
			var createdBytes = Encoding.UTF8.GetBytes(created);
			var passwordBytes = Encoding.UTF8.GetBytes(password);
			var combined = new byte[createdBytes.Length + nonce.Length + passwordBytes.Length];
			Buffer.BlockCopy(nonce, 0, combined, 0, nonce.Length);
			Buffer.BlockCopy(createdBytes, 0, combined, nonce.Length, createdBytes.Length);
			Buffer.BlockCopy(passwordBytes, 0, combined, nonce.Length + createdBytes.Length, passwordBytes.Length);

			return Convert.ToBase64String(SHA1.Create().ComputeHash(combined));
		}
	}
}
