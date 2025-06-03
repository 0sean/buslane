class ApiKey < ApplicationRecord
    before_create :generate_raw_token
    before_create :generate_token_digest

    attr_accessor :raw_token

    def self.find_by_token!(token)
        find_by!(token_digest: generate_digest(token))
    end

    def self.find_by_token(token)
        find_by(token_digest: generate_digest(token))
    end

    def self.generate_digest(token)
        OpenSSL::HMAC.hexdigest(OpenSSL::Digest.new("sha256"), Rails.application.secret_key_base, token)
    end

    private

    def generate_raw_token
        self.raw_token = SecureRandom.base64(32)
    end

    def generate_token_digest
        self.token_digest = self.class.generate_digest(raw_token)
    end
end
