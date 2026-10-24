import { authenticationJSON, b64urlToBytes, bytesToB64url, creationOptions, prfSalt, registrationJSON, requestOptions } from "./webauthn";

describe("base64url", () => {
  it("fait l'aller-retour sur toutes les longueurs et tous les octets", () => {
    for (let n = 0; n < 70; n++) {
      const bytes = new Uint8Array(n).map((_, i) => (i * 37 + n) & 0xff);
      const s = bytesToB64url(bytes);
      expect(s).not.toMatch(/[+/=]/);
      expect(Array.from(b64urlToBytes(s))).toEqual(Array.from(bytes));
    }
  });

  it("accepte le remplissage éventuel", () => {
    expect(Array.from(b64urlToBytes("AQI="))).toEqual([1, 2]);
  });
});

const SALT = bytesToB64url(new Uint8Array(32).fill(7));

describe("options du serveur", () => {
  it("convertit les options d'inscription", () => {
    const o = creationOptions({
      rp: { id: "localhost", name: "Phonalys" },
      user: { id: "AQID", name: "praticien", displayName: "praticien" },
      challenge: "BAUG",
      pubKeyCredParams: [{ type: "public-key", alg: -7 }],
      authenticatorSelection: { userVerification: "required" },
      extensions: { prf: {} },
      excludeCredentials: [{ type: "public-key", id: "CQo" }],
    });
    expect(Array.from(o.challenge as Uint8Array)).toEqual([4, 5, 6]);
    expect(Array.from(o.user.id as Uint8Array)).toEqual([1, 2, 3]);
    expect(Array.from(o.excludeCredentials?.[0]?.id as Uint8Array)).toEqual([9, 10]);
    expect(o.authenticatorSelection?.userVerification).toBe("required");
  });

  it("transmet le sel PRF seulement s'il est demandé", () => {
    const base = { rpId: "localhost", challenge: "AQID", allowCredentials: [{ type: "public-key", id: "CQo" }] };
    expect(requestOptions(base).extensions).toBeUndefined();
    expect(prfSalt(base)).toBeUndefined();

    const withPrf = { ...base, extensions: { prf: { eval: { first: SALT } } } };
    const ext = requestOptions(withPrf).extensions as { prf: { eval: { first: Uint8Array } } };
    expect(ext.prf.eval.first).toHaveLength(32);
    expect(prfSalt(withPrf)).toBe(SALT);
  });

  it("refuse un défi manquant", () => {
    expect(() => requestOptions({ rpId: "localhost" })).toThrow(/challenge/);
  });
});

function fakeCredential(response: object, ext: object) {
  return {
    rawId: new Uint8Array([1, 2, 3]).buffer,
    response,
    getClientExtensionResults: () => ext,
  } as unknown as PublicKeyCredential;
}

describe("réponses du navigateur", () => {
  it("sérialise une signature et la sortie PRF", () => {
    const cred = fakeCredential(
      {
        clientDataJSON: new Uint8Array([1]).buffer,
        authenticatorData: new Uint8Array([2]).buffer,
        signature: new Uint8Array([3]).buffer,
      },
      { prf: { results: { first: new Uint8Array(32).fill(1).buffer } } },
    );
    const { response, prfOutput } = authenticationJSON(cred);
    expect(response).toEqual({ id: "AQID", clientDataJSON: "AQ", authenticatorData: "Ag", signature: "Aw" });
    expect(b64urlToBytes(prfOutput as string)).toHaveLength(32);
  });

  it("signale l'absence de PRF", () => {
    const cred = fakeCredential(
      { clientDataJSON: new ArrayBuffer(1), authenticatorData: new ArrayBuffer(1), signature: new ArrayBuffer(1) },
      {},
    );
    expect(authenticationJSON(cred).prfOutput).toBeUndefined();
    const reg = fakeCredential({ clientDataJSON: new ArrayBuffer(1), attestationObject: new ArrayBuffer(1) }, { prf: { enabled: false } });
    expect(registrationJSON(reg).prfSupported).toBe(false);
  });
});
